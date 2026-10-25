//! LilyPond's own description of the name under the cursor.

use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};
use tree_sitter::Node;

use crate::commanddata::{CommandData, Function, IdentifierKind, Scope};
use crate::completion::{in_comment_or_string, is_word, word_start};
use crate::document::Document;
use crate::node_text;

/// The description of the command, markup command, grob, property or context at byte `pos` of
/// `doc`, from LilyPond's command data.
pub fn hover(doc: &Document, pos: usize, data: Option<&CommandData>) -> Option<Hover> {
    let data = data?;
    let text = &doc.text;
    if pos > text.len() || !text.is_char_boundary(pos) {
        return None;
    }
    let start = word_start(text, pos);
    let end = text[pos..]
        .find(|c: char| !is_word(c))
        .map_or(text.len(), |i| pos + i);
    if start == end || in_comment_or_string(doc, start + 1) {
        return None;
    }
    let word = &text[start..end];
    let node = doc.tree.root_node().descendant_for_byte_range(start, end)?;
    let value = if text[..start].ends_with('\\') {
        command(word, node, data)
    } else {
        name(word, node, text, data)
    }?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    })
}

/// A `\` command: a markup command in markup, and otherwise a music function, one of LilyPond's
/// variables, or a context definition such as `\Staff`.
fn command(word: &str, node: Node, data: &CommandData) -> Option<String> {
    if in_markup(node) {
        return data.markup(word).map(function);
    }
    if let Some(found) = data.music_function(word) {
        return Some(function(found));
    }
    if let Some(kind) = data.identifier(word) {
        let kind = match kind {
            IdentifierKind::Music => "music",
            IdentifierKind::Markup => "markup",
            IdentifierKind::Number => "number",
            IdentifierKind::String => "string",
            IdentifierKind::ContextDef => "context definition",
            IdentifierKind::Scheme => "Scheme value",
        };
        return Some(format!("`\\{word}` ({kind} variable)"));
    }
    data.context(word)
        .map(|c| described(word, "context", c.doc.as_deref()))
}

/// A word without a `\`: part of a `\override` or `\set` path, the context of a `\new`, or a grob
/// or context named elsewhere, such as the `Stem` in `\omit Stem`.
fn name(word: &str, node: Node, text: &str, data: &CommandData) -> Option<String> {
    if let Some(path) = ancestor(node, "property_path")
        && let Some(command) = path.parent()
    {
        let mut cursor = path.walk();
        let segments: Vec<Node> = path
            .named_children(&mut cursor)
            .filter(|n| n.kind() == "identifier")
            .collect();
        let at = segments.iter().position(|s| s.id() == node.id())?;
        let names: Vec<&str> = segments.iter().map(|s| node_text::text(*s, text)).collect();
        let context_first = names.len() > 1 && data.context(names[0]).is_some();
        return match command.kind() {
            "override" | "revert" => match (context_first, at) {
                (true, 0) => context(word, data),
                (true, 1) | (false, 0) => grob(word, data),
                _ => property(word, Scope::Backend, data),
            },
            "set" | "unset" => match (context_first, at) {
                (true, 0) => context(word, data),
                _ => property(word, Scope::Context, data),
            },
            _ => None,
        };
    }
    grob(word, data).or_else(|| context(word, data))
}

fn context(word: &str, data: &CommandData) -> Option<String> {
    data.context(word)
        .map(|c| described(word, "context", c.doc.as_deref()))
}

fn grob(word: &str, data: &CommandData) -> Option<String> {
    data.grob(word)
        .map(|g| described(word, "grob", g.doc.as_deref()))
}

fn property(word: &str, scope: Scope, data: &CommandData) -> Option<String> {
    let property = data.property(word, scope)?;
    let what = match scope {
        Scope::Backend => "grob property",
        Scope::Context => "context property",
    };
    let kind = format!("{what}, `{}`", property.pred);
    Some(described(word, &kind, property.doc.as_deref()))
}

/// A function's call shape in a code block, such as `\relative [ly:pitch?] ly:music?` with the
/// arguments that can be left out in brackets, then its description.
fn function(function: &Function) -> String {
    let mut shape = format!("\\{}", function.name);
    for arg in &function.args {
        let pred = if arg.pred.is_empty() {
            "any"
        } else {
            &arg.pred
        };
        if arg.optional {
            shape.push_str(&format!(" [{pred}]"));
        } else {
            shape.push_str(&format!(" {pred}"));
        }
    }
    with_doc(
        format!("```lilypond\n{shape}\n```"),
        function.doc.as_deref(),
    )
}

fn described(word: &str, kind: &str, doc: Option<&str>) -> String {
    with_doc(format!("`{word}` ({kind})"), doc)
}

fn with_doc(heading: String, doc: Option<&str>) -> String {
    match doc {
        Some(doc) if !doc.is_empty() => format!("{heading}\n\n{doc}"),
        _ => heading,
    }
}

/// Whether `node` is in markup, and not in a score that markup holds.
fn in_markup(node: Node) -> bool {
    let mut up = node.parent();
    while let Some(n) = up {
        match n.kind() {
            "markup" | "markup_list" => return true,
            "score_block" | "embedded_lilypond" => return false,
            _ => up = n.parent(),
        }
    }
    false
}

/// `node`, or the nearest node above it, of kind `kind`.
fn ancestor<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut at = Some(node);
    while let Some(n) = at {
        if n.kind() == kind {
            return Some(n);
        }
        at = n.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use lsp_types::{HoverContents, MarkupKind};

    use super::*;
    use crate::testing::{document, mini};

    /// Where the cursor is in a test's source. `|` would be a bar check.
    const CURSOR: char = '‸';

    /// The Markdown `hover` gives in an untitled document holding `source`, at its cursor.
    fn hovered(source: &str) -> Option<String> {
        let pos = source.find(CURSOR).expect("a cursor in the source");
        let doc = document(None, &source.replacen(CURSOR, "", 1));
        let data = mini();
        hover(&doc, pos, Some(&data)).map(|h| match h.contents {
            HoverContents::Markup(m) if m.kind == MarkupKind::Markdown => m.value,
            other => panic!("not Markdown: {other:?}"),
        })
    }

    fn shows(source: &str, wanted: &str) {
        let shown = hovered(source).unwrap_or_default();
        assert!(
            shown.contains(wanted),
            "{source}: {shown:?} lacks {wanted:?}"
        );
    }

    #[test]
    fn hover_shows_doc() {
        shows(
            "{ \\rel‸ative c' { c } }",
            "\\relative [ly:pitch?] ly:music?",
        );
        shows(
            "{ \\rel‸ative c' { c } }",
            "Notes in the music take their octave from the note before.",
        );
        shows("\\markup { \\bo‸ld x }", "\\bold markup?");
        shows("\\markup { \\bo‸ld x }", "Heavy type.");
        shows(
            "{ \\override Note‸Head.color = #red }",
            "The head of a note.",
        );
        shows(
            "{ \\override NoteHead.co‸lor = #red }",
            "The colour to draw in.",
        );
        shows("{ \\override Sta‸ff.NoteHead.color = #red }", "One staff.");
        shows(
            "{ \\set Staff.auto‸Beaming = ##f }",
            "Beams notes without being asked.",
        );
        shows(
            "{ \\set Staff.co‸lor = #red }",
            "A context property that shares a backend name.",
        );
        shows("\\new Sta‸ff { c }", "One staff.");
        shows("{ \\omit St‸em c }", "The line from a note head.");
        shows("{ c\\ferm‸ata }", "music");
    }

    #[test]
    fn hover_on_unknown_is_none() {
        assert_eq!(hovered("{ \\melo‸dyy }"), None);
        assert_eq!(hovered("{ c ‸ d }"), None);
        assert_eq!(hovered("{ ci‸s }"), None);
        assert_eq!(hovered("% \\rel‸ative"), None);
        let doc = document(None, "{ \\relative { c } }");
        assert_eq!(hover(&doc, 4, None), None);
    }
}
