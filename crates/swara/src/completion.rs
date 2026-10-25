//! What to offer at the cursor: commands after `\`, markup commands in markup, contexts, grobs and
//! properties in a `\new`, `\set` or `\override`, and note names in music.

use std::collections::{BTreeSet, HashSet};

use lsp_types::{CompletionItem, CompletionItemKind, CompletionTextEdit, Range, TextEdit};

use crate::commanddata::{Arg, ArgKind, CommandData, IdentifierKind, Scope};
use crate::diagnostics::names::{KEYWORDS, Table, place_at};
use crate::document::Document;
use crate::index::{Index, visible_definitions};
use crate::line_index::Encoding;
use crate::node_text;

/// The completions at byte `pos` of `doc`. Each one replaces the word before the cursor, from its
/// `\` when it has one. Every name that fits the place is returned, and the editor narrows them
/// to the ones matching what has been typed.
pub fn complete(
    doc: &Document,
    pos: usize,
    data: Option<&CommandData>,
    index: &Index,
    enc: Encoding,
) -> Vec<CompletionItem> {
    let text = &doc.text;
    if pos > text.len() || !text.is_char_boundary(pos) || in_comment_or_string(doc, pos) {
        return Vec::new();
    }
    let start = word_start(text, pos);
    if text[..start].ends_with('\\') {
        let mut offers = Offers::new(doc, start - 1, pos, enc);
        commands(&mut offers, doc, pos, data, index);
        return offers.items;
    }
    let mut offers = Offers::new(doc, start, pos, enc);
    let Some(data) = data else {
        return Vec::new();
    };
    if text[..start].ends_with('.') {
        path(&mut offers, text, start, data);
        return offers.items;
    }
    match command_before(text, start) {
        Some("new" | "context") => offers.contexts(data),
        Some("set" | "unset") => {
            offers.contexts(data);
            offers.properties(data);
        }
        Some("override" | "revert") => {
            offers.contexts(data);
            offers.grobs(data);
        }
        _ if start < pos => pitches(&mut offers, doc, start, pos, data, index),
        _ => {}
    }
    offers.items
}

/// The names that can follow a `\`: markup commands in markup, and music functions, LilyPond's
/// own variables and the keywords elsewhere, with the variables the file can see in both.
fn commands(
    offers: &mut Offers,
    doc: &Document,
    pos: usize,
    data: Option<&CommandData>,
    index: &Index,
) {
    // A variable of the file's own wins over one of LilyPond's with the same name.
    for definition in visible_definitions(doc, index) {
        let (kind, detail) = match &definition.signature {
            Some(args) => (CompletionItemKind::FUNCTION, signature(args)),
            None => (CompletionItemKind::VARIABLE, None),
        };
        offers.add(node_text::spelled(&definition.name), kind, detail);
    }
    let Some(data) = data else {
        offers.keywords();
        return;
    };
    let in_markup = place_at(doc, pos, data, index).in_markup;
    if in_markup {
        for function in data.markups() {
            let detail = signature(&function.args);
            offers.add(
                node_text::spelled(&function.name),
                CompletionItemKind::FUNCTION,
                detail,
            );
        }
    } else {
        for function in data.music_functions() {
            let detail = signature(&function.args);
            offers.add(
                node_text::spelled(&function.name),
                CompletionItemKind::FUNCTION,
                detail,
            );
        }
        offers.keywords();
    }
    for (name, kind) in data.identifiers() {
        if (kind == IdentifierKind::Markup) == in_markup {
            offers.add(node_text::spelled(name), CompletionItemKind::CONSTANT, None);
        }
    }
}

/// The next part of a dotted `\override` or `\set` path, such as the grob after `Staff.` or the
/// property after `Staff.NoteHead.`.
fn path(offers: &mut Offers, text: &str, start: usize, data: &CommandData) {
    let mut segments = Vec::new();
    let mut end = start;
    while text[..end].ends_with('.') {
        let from = word_start(text, end - 1);
        if from == end - 1 {
            return;
        }
        segments.insert(0, &text[from..end - 1]);
        end = from;
    }
    let is_context = |name: &str| data.context(name).is_some();
    match (command_before(text, end), segments.as_slice()) {
        (Some("override" | "revert"), [context]) if is_context(context) => offers.grobs(data),
        (Some("override" | "revert"), [grob]) => offers.grob_properties(data, grob),
        (Some("override" | "revert"), [context, grob]) if is_context(context) => {
            offers.grob_properties(data, grob)
        }
        (Some("set" | "unset"), [context]) if is_context(context) => offers.properties(data),
        _ => {}
    }
}

/// The note or drum names of the place the word at `start..pos` is in, when a note can go there.
fn pitches(
    offers: &mut Offers,
    doc: &Document,
    start: usize,
    pos: usize,
    data: &CommandData,
    index: &Index,
) {
    if !doc.text[start..pos].chars().all(char::is_alphabetic) {
        return;
    }
    let place = place_at(doc, pos, data, index);
    if place.argument || place.top_level {
        return;
    }
    let names: BTreeSet<&String> = match place.table {
        Table::Pitches => {
            let Some(languages) = &place.languages else {
                return;
            };
            languages
                .iter()
                .filter_map(|language| data.pitches(language))
                .flatten()
                .collect()
        }
        Table::Drums => data.drums().iter().collect(),
        Table::Off => return,
    };
    for name in names {
        offers.add(name.clone(), CompletionItemKind::VALUE, None);
    }
}

/// The items gathered so far, each replacing `from..to`, and their labels.
struct Offers {
    range: Range,
    seen: HashSet<String>,
    items: Vec<CompletionItem>,
}

impl Offers {
    fn new(doc: &Document, from: usize, to: usize, enc: Encoding) -> Self {
        let at = |byte| doc.index.byte_to_position(&doc.text, byte, enc);
        Offers {
            range: Range::new(at(from), at(to)),
            seen: HashSet::new(),
            items: Vec::new(),
        }
    }

    /// Adds `label` unless an item with that label is already offered.
    fn add(&mut self, label: String, kind: CompletionItemKind, detail: Option<String>) {
        if !self.seen.insert(label.clone()) {
            return;
        }
        let edit = TextEdit::new(self.range, label.clone());
        self.items.push(CompletionItem {
            label,
            kind: Some(kind),
            detail,
            text_edit: Some(CompletionTextEdit::Edit(edit)),
            ..CompletionItem::default()
        });
    }

    fn keywords(&mut self) {
        for keyword in KEYWORDS {
            self.add(format!("\\{keyword}"), CompletionItemKind::KEYWORD, None);
        }
    }

    fn contexts(&mut self, data: &CommandData) {
        for context in data.contexts() {
            self.add(context.name.clone(), CompletionItemKind::CLASS, None);
        }
    }

    fn grobs(&mut self, data: &CommandData) {
        for grob in data.grobs() {
            self.add(grob.name.clone(), CompletionItemKind::CLASS, None);
        }
    }

    fn properties(&mut self, data: &CommandData) {
        for property in data.properties(Scope::Context) {
            let pred = Some(property.pred.clone());
            self.add(property.name.clone(), CompletionItemKind::PROPERTY, pred);
        }
    }

    /// The properties of every interface `grob` has.
    fn grob_properties(&mut self, data: &CommandData, grob: &str) {
        let Some(grob) = data.grob(grob) else { return };
        for interface in grob.interfaces.iter().filter_map(|i| data.interface(i)) {
            for name in &interface.props {
                let pred = data.property(name, Scope::Backend).map(|p| p.pred.clone());
                self.add(name.clone(), CompletionItemKind::PROPERTY, pred);
            }
        }
    }
}

/// What a function takes, such as `pitch? music` for `\relative`, where `?` marks an argument
/// that can be left out.
fn signature(args: &[Arg]) -> Option<String> {
    let words: Vec<String> = args
        .iter()
        .map(|arg| {
            let word = match arg.kind() {
                ArgKind::Pitch => "pitch",
                ArgKind::Duration => "duration",
                ArgKind::Music => "music",
                ArgKind::Word => "word",
                ArgKind::Markup => "markup",
                ArgKind::Number => "number",
                ArgKind::Scheme => "scheme",
            };
            if arg.optional {
                format!("{word}?")
            } else {
                word.to_owned()
            }
        })
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

/// Whether a character can be part of a command, context, grob, property or note name.
pub(crate) fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

/// Where the word ending at byte `pos` of `text` starts.
pub(crate) fn word_start(text: &str, pos: usize) -> usize {
    text[..pos]
        .char_indices()
        .rev()
        .find(|&(_, c)| !is_word(c))
        .map_or(0, |(i, c)| i + c.len_utf8())
}

/// The command a word at byte `start` follows, past any spaces: `override` for
/// `\override NoteHead`.
fn command_before(text: &str, start: usize) -> Option<&str> {
    let before = text[..start].trim_end();
    let from = word_start(before, before.len());
    (from < before.len() && before[..from].ends_with('\\')).then(|| &before[from..])
}

/// Whether the character before byte `pos` of `doc` is inside a comment or a string, where
/// nothing is a name.
pub(crate) fn in_comment_or_string(doc: &Document, pos: usize) -> bool {
    if pos == 0 {
        return false;
    }
    let root = doc.tree.root_node();
    let mut node = root.descendant_for_byte_range(pos - 1, pos);
    while let Some(n) = node {
        let body = node_text::text(n, &doc.text);
        let closed = |closer: &str| body.len() > closer.len() && body.ends_with(closer);
        let inside = match n.kind() {
            "line_comment" | "scheme_comment" => true,
            "block_comment" => pos < n.end_byte() || !closed("%}"),
            "scheme_block_comment" => pos < n.end_byte() || !closed("|#"),
            "scheme_datum_comment" => pos < n.end_byte(),
            "string" => pos < n.end_byte() || !closed("\""),
            _ => false,
        };
        if inside {
            return true;
        }
        node = n.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use lsp_types::{CompletionTextEdit, Position, Range, TextEdit};

    use super::*;
    use crate::testing::{Scratch, document, mini};

    /// Where the cursor is in a test's source. `|` would be a bar check.
    const CURSOR: char = '‸';

    /// `source` without its cursor, and the cursor's byte offset.
    fn split(source: &str) -> (String, usize) {
        let pos = source.find(CURSOR).expect("a cursor in the source");
        (source.replacen(CURSOR, "", 1), pos)
    }

    /// What `complete` offers in an untitled document holding `source`, at its cursor.
    fn offered(source: &str) -> Vec<CompletionItem> {
        let (text, pos) = split(source);
        let doc = document(None, &text);
        let data = mini();
        complete(
            &doc,
            pos,
            Some(&data),
            &Index::new(Vec::new()),
            Encoding::Utf8,
        )
    }

    fn labels(source: &str) -> Vec<String> {
        let mut labels: Vec<String> = offered(source).into_iter().map(|i| i.label).collect();
        labels.sort();
        labels
    }

    fn has(labels: &[String], label: &str) -> bool {
        labels.iter().any(|l| l == label)
    }

    fn edit(start: (u32, u32), end: (u32, u32), text: &str) -> Option<CompletionTextEdit> {
        let range = Range::new(Position::new(start.0, start.1), Position::new(end.0, end.1));
        Some(CompletionTextEdit::Edit(TextEdit::new(range, text.into())))
    }

    #[test]
    fn backslash_offers_functions_and_variables() {
        let items = offered("melody = { c }\n{ \\rel‸ }");
        let relative = items
            .iter()
            .find(|i| i.label == "\\relative")
            .expect("`\\relative` offered");
        assert_eq!(relative.detail.as_deref(), Some("pitch? music"));
        assert_eq!(relative.text_edit, edit((1, 2), (1, 6), "\\relative"));
        let labels: Vec<String> = items.into_iter().map(|i| i.label).collect();
        for label in [
            "\\melody",
            "\\transpose",
            "\\fermata",
            "\\major",
            "\\RemoveEmptyStaves",
            "\\score",
        ] {
            assert!(has(&labels, label), "{label} missing");
        }
        for label in ["\\bold", "\\whiteTriangleMarkup", "\\Staff", "Staff"] {
            assert!(!has(&labels, label), "{label} offered");
        }
    }

    #[test]
    fn punctuation_names_offered_with_one_backslash() {
        let labels = labels("\"\\\\{\" = { c }\n{ \\‸ }");
        assert!(has(&labels, "\\^"), "`\\^` missing");
        assert!(has(&labels, "\\{"), "`\\{{` missing");
        let doubled: Vec<&String> = labels.iter().filter(|l| l.starts_with("\\\\")).collect();
        assert!(doubled.is_empty(), "offered {doubled:?}");
    }

    #[test]
    fn markup_offers_markup_commands() {
        let labels = labels("\\markup { \\bo‸ }");
        for label in [
            "\\bold",
            "\\with-color",
            "\\wordwrap-lines",
            "\\whiteTriangleMarkup",
        ] {
            assert!(has(&labels, label), "{label} missing");
        }
        for label in ["\\relative", "\\fermata", "\\score"] {
            assert!(!has(&labels, label), "{label} offered");
        }
        let fontsize = offered("\\markup \\fo‸")
            .into_iter()
            .find(|i| i.label == "\\fontsize");
        assert_eq!(
            fontsize.and_then(|i| i.detail).as_deref(),
            Some("number markup")
        );
    }

    #[test]
    fn new_offers_contexts() {
        let contexts = ["DrumStaff", "Score", "Staff", "Voice"];
        assert_eq!(labels("{ \\new St‸ }"), contexts);
        assert_eq!(labels("\\new ‸"), contexts);
        assert_eq!(labels("<< \\context Vo‸ >>"), contexts);
        let staff = offered("{ \\new St‸ }")
            .into_iter()
            .find(|i| i.label == "Staff");
        assert_eq!(
            staff.and_then(|i| i.text_edit),
            edit((0, 7), (0, 9), "Staff")
        );
    }

    #[test]
    fn override_path() {
        assert_eq!(
            labels("{ \\override Staff.Not‸ }"),
            ["NoteHead", "StaffSymbol", "Stem"]
        );
        assert_eq!(
            labels("{ \\override NoteHead.co‸ }"),
            ["color", "style", "transparent"]
        );
        assert_eq!(
            labels("{ \\once \\override Staff.Stem.‸ }"),
            ["color", "direction", "length", "transparent"]
        );
        assert_eq!(
            labels("{ \\revert No‸ }"),
            [
                "DrumStaff",
                "NoteHead",
                "Score",
                "Staff",
                "StaffSymbol",
                "Stem",
                "Voice"
            ]
        );
        assert!(labels("{ \\override Nothing.co‸ }").is_empty());
        let color = offered("{ \\override NoteHead.co‸ }")
            .into_iter()
            .find(|i| i.label == "color");
        assert_eq!(
            color.and_then(|i| i.text_edit),
            edit((0, 21), (0, 23), "color")
        );
    }

    #[test]
    fn set_path() {
        assert_eq!(
            labels("{ \\set Staff.auto‸ }"),
            ["autoBeaming", "color", "instrumentName"]
        );
        assert_eq!(
            labels("{ \\unset St‸ }"),
            [
                "DrumStaff",
                "Score",
                "Staff",
                "Voice",
                "autoBeaming",
                "color",
                "instrumentName"
            ]
        );
    }

    #[test]
    fn pitches_follow_language() {
        assert!(has(&labels("{ ci‸ }"), "cis"));
        let english = labels("\\language \"english\"\n{ c‸ }");
        assert!(has(&english, "cs") && !has(&english, "cis"), "{english:?}");
        assert_eq!(labels("\\drummode { b‸ }"), ["bd", "hh", "sn"]);
        assert!(labels("{ \\clef tre‸ }").is_empty());
        assert!(labels("\\lyricmode { la‸ }").is_empty());
        // A word at the top level names an assignment.
        assert!(labels("mel‸").is_empty());
    }

    #[test]
    fn nothing_in_comments_or_strings() {
        assert!(offered("% \\rel‸").is_empty());
        assert!(offered("%{ \\rel‸ %}").is_empty());
        assert!(offered("{ c^\"\\rel‸\" }").is_empty());
    }

    #[test]
    fn variables_from_includes() {
        let dir = Scratch::new("completion-includes");
        dir.write("defs.ily", "tune = { c }\n");
        let (text, pos) = split("\\include \"defs.ily\"\n{ \\tu‸ }\n");
        let path = dir.path().join("main.ly");
        let doc = document(Some(&path), &text);
        // The server holds every open file in the index.
        let mut index = Index::new(Vec::new());
        index.update(&path, &doc);
        let data = mini();
        let items = complete(&doc, pos, Some(&data), &index, Encoding::Utf8);
        assert!(items.iter().any(|i| i.label == "\\tune"));
    }

    #[test]
    fn offers_without_command_data() {
        let (text, pos) = split("melody = { c }\n{ \\‸ }");
        let doc = document(None, &text);
        let labels: Vec<String> =
            complete(&doc, pos, None, &Index::new(Vec::new()), Encoding::Utf8)
                .into_iter()
                .map(|i| i.label)
                .collect();
        assert!(
            has(&labels, "\\melody") && has(&labels, "\\score"),
            "{labels:?}"
        );
        assert!(!has(&labels, "\\relative"));
    }
}
