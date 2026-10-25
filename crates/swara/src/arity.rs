//! Which nodes after a music function are its arguments, and what kind each one is. The grammar
//! cannot know a function's signature, so a command and its arguments are flat siblings in the
//! tree, and this matches them up from LilyPond's command data.

use std::collections::HashMap;

use tree_sitter::Node;

use crate::commanddata::{Arg, ArgKind, CommandData, IdentifierKind};
use crate::node_text;

/// Nested functions deeper than this stop binding, so a pathological file cannot overflow the
/// stack.
const MAX_DEPTH: usize = 64;

/// The signatures a binding can see: LilyPond's own functions, and the ones the files define.
pub struct Signatures<'a> {
    pub data: &'a CommandData,
    pub own: &'a HashMap<String, Vec<Arg>>,
}

/// The arguments `cmd` takes from `siblings`, the nodes after it in the same parent, each with
/// the kind its signature asks for. A name that is not a music function binds nothing.
// The checks and completion bind through `bind_with`, which sees the files' own functions too.
#[cfg(test)]
pub fn bind_args<'t>(
    cmd: &str,
    siblings: &[Node<'t>],
    text: &str,
    data: &CommandData,
) -> Vec<(Node<'t>, ArgKind)> {
    let own = HashMap::new();
    bind_with(cmd, siblings, text, &Signatures { data, own: &own })
}

/// `bind_args` with the functions the files define as well.
pub fn bind_with<'t>(
    cmd: &str,
    siblings: &[Node<'t>],
    text: &str,
    sigs: &Signatures,
) -> Vec<(Node<'t>, ArgKind)> {
    let mut bound = Vec::new();
    if let Some(args) = sigs.function(cmd) {
        bind(args, siblings, text, sigs, &mut bound, 0);
    }
    bound
}

/// Nodes that sit between a function and its arguments without being one: comments, and the
/// post-events that attach to the note before them.
const POST_EVENTS: &[&str] = &[
    "articulation",
    "fingering",
    "text_script",
    "tie",
    "slur_start",
    "slur_end",
    "phrasing_slur_start",
    "phrasing_slur_end",
    "beam_start",
    "beam_end",
    "hairpin",
    "hairpin_end",
    "ligature_start",
    "ligature_end",
    "string_number",
    "tremolo",
];

/// The nodes that are music on their own.
const MUSIC: &[&str] = &[
    "note",
    "chord",
    "rest",
    "skip",
    "multi_measure_rest",
    "chord_repetition",
    "sequential_music",
    "simultaneous_music",
    "new_context",
    "override",
    "set",
    "revert",
    "unset",
    "repeat",
    "tempo",
    "change",
    "lyric_mode",
    "lyrics_to",
    "add_lyrics",
    "chord_mode",
    "figure_mode",
    "drum_mode",
    "note_mode",
];

impl Signatures<'_> {
    fn function(&self, name: &str) -> Option<&[Arg]> {
        match self.own.get(name) {
            Some(args) => Some(args),
            None => self.data.music_function(name).map(|f| f.args.as_slice()),
        }
    }
}

/// Binds `args` from the front of `siblings` and returns how many siblings that consumed. A
/// function bound as an argument takes its own arguments next.
fn bind<'t>(
    args: &[Arg],
    siblings: &[Node<'t>],
    text: &str,
    sigs: &Signatures,
    bound: &mut Vec<(Node<'t>, ArgKind)>,
    depth: usize,
) -> usize {
    let mut at = 0;
    for arg in args {
        while siblings
            .get(at)
            .is_some_and(|n| n.is_extra() || POST_EVENTS.contains(&n.kind()))
        {
            at += 1;
        }
        let Some(&node) = siblings.get(at) else { break };
        let kind = arg.kind();
        if !fits(node, kind, arg.optional, text, sigs) {
            if arg.optional {
                continue;
            }
            break;
        }
        bound.push((node, kind));
        at += 1;
        if node.kind() == "command"
            && depth < MAX_DEPTH
            && let Some(inner) = sigs.function(&node_text::command(node, text))
        {
            at += bind(inner, &siblings[at..], text, sigs, bound, depth + 1);
        }
    }
    at
}

/// Whether `node` can be an argument of `kind`.
fn fits(node: Node, kind: ArgKind, optional: bool, text: &str, sigs: &Signatures) -> bool {
    use ArgKind::*;
    let bare_note = || node.kind() == "note" && node.child_by_field_name("duration").is_none();
    match node.kind() {
        // Scheme can produce anything, so the signature decides.
        "scheme" => true,
        "command" => {
            let name = node_text::command(node, text);
            if name == "default" {
                return optional;
            }
            if sigs.function(&name).is_some() {
                return matches!(kind, Music | Scheme);
            }
            match sigs.data.identifier(&name) {
                Some(IdentifierKind::Music) => matches!(kind, Music | Scheme),
                Some(IdentifierKind::Number) => matches!(kind, Number | Scheme),
                Some(IdentifierKind::String) => matches!(kind, Word | Markup | Scheme),
                Some(IdentifierKind::Markup) => matches!(kind, Markup | Scheme),
                Some(IdentifierKind::ContextDef) => kind == Scheme,
                Some(IdentifierKind::Scheme) => true,
                // A variable the files define could hold anything, but an optional argument
                // passes it by unless it is plainly music.
                None => !optional || matches!(kind, Music | Scheme),
            }
        }
        "dotted_name" => matches!(kind, Word | Music | Scheme),
        other => match kind {
            Pitch => bare_note(),
            Duration => matches!(other, "duration" | "number"),
            Music => MUSIC.contains(&other),
            Word => matches!(other, "string" | "identifier") || bare_note(),
            Markup => matches!(other, "markup" | "markup_list" | "string") || bare_note(),
            Number => matches!(other, "number" | "fraction" | "number_list"),
            Scheme => !optional,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::testing::{document, first, mini};

    /// What the first command in `source` binds, as each argument's text and kind.
    fn bound(source: &str) -> Vec<(String, ArgKind)> {
        let data = mini();
        let doc = document(None, source);
        let cmd = first(doc.tree.root_node(), "command").unwrap();
        let mut siblings = Vec::new();
        let mut next = cmd.next_named_sibling();
        while let Some(node) = next {
            siblings.push(node);
            next = node.next_named_sibling();
        }
        let name = node_text::command(cmd, &doc.text);
        bind_args(&name, &siblings, &doc.text, &data)
            .into_iter()
            .map(|(node, kind)| (node_text::text(node, &doc.text).to_owned(), kind))
            .collect()
    }

    fn arg(text: &str, kind: ArgKind) -> (String, ArgKind) {
        (text.to_owned(), kind)
    }

    #[test]
    fn relative_skips_its_optional_pitch() {
        assert_eq!(
            bound(r"{ \relative { c } }"),
            [arg("{ c }", ArgKind::Music)]
        );
        assert_eq!(
            bound(r"{ \relative c' { c } }"),
            [arg("c'", ArgKind::Pitch), arg("{ c }", ArgKind::Music)]
        );
    }

    #[test]
    fn clef_binds_a_word() {
        assert_eq!(bound(r"{ \clef treble c }"), [arg("treble", ArgKind::Word)]);
    }

    #[test]
    fn nested_function_consumes_its_own_args() {
        assert_eq!(
            bound(r"{ \once \omit Stem c }"),
            [arg(r"\omit", ArgKind::Music), arg("Stem", ArgKind::Scheme)]
        );
    }

    #[test]
    fn required_mismatch_stops() {
        assert_eq!(bound(r"{ \clef { c } }"), []);
    }

    #[test]
    fn comments_and_post_events_skipped() {
        assert_eq!(
            bound("{ \\transpose c % a comment\n d-> { e } }"),
            [
                arg("c", ArgKind::Pitch),
                arg("d", ArgKind::Pitch),
                arg("{ e }", ArgKind::Music)
            ]
        );
    }

    #[test]
    fn default_fills_only_an_optional_slot() {
        assert_eq!(
            bound(r"{ \key d \default }"),
            [arg("d", ArgKind::Pitch), arg(r"\default", ArgKind::Scheme)]
        );
        assert_eq!(bound(r"{ \clef \default }"), []);
    }

    #[test]
    fn own_signatures_bind() {
        let data = mini();
        let doc = document(None, r"{ \fn treble { c } }");
        let cmd = first(doc.tree.root_node(), "command").unwrap();
        let siblings: Vec<Node> =
            std::iter::successors(cmd.next_named_sibling(), |n| n.next_named_sibling()).collect();
        let own = HashMap::from([(
            "fn".to_owned(),
            vec![
                Arg {
                    pred: "symbol?".into(),
                    optional: false,
                },
                Arg {
                    pred: "ly:music?".into(),
                    optional: false,
                },
            ],
        )]);
        let kinds: Vec<ArgKind> = bind_with(
            "fn",
            &siblings,
            &doc.text,
            &Signatures {
                data: &data,
                own: &own,
            },
        )
        .into_iter()
        .map(|(_, kind)| kind)
        .collect();
        assert_eq!(kinds, [ArgKind::Word, ArgKind::Music]);
    }
}
