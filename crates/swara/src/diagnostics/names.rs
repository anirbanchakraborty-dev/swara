//! Commands nothing defines, and note names the active language does not have.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use lsp_types::Diagnostic;
use tree_sitter::Node;

use super::error;
use crate::arity::{Signatures, bind_with};
use crate::commanddata::{ArgKind, CommandData};
use crate::document::Document;
use crate::file_uri;
use crate::index::{DEFAULT_LANGUAGE, Index, Languages, summarize};
use crate::line_index::Encoding;
use crate::node_text;

/// Words LilyPond's lexer and parser handle themselves, which no command data lists: keywords,
/// the units after a number in `\paper`, and a few names its own files use.
const KEYWORDS: &[&str] = &[
    "accepts",
    "addlyrics",
    "alias",
    "alternative",
    "book",
    "bookpart",
    "bp",
    "breve",
    "change",
    "chordmode",
    "chords",
    "cm",
    "consists",
    "context",
    "defaultchild",
    "denies",
    "description",
    "drummode",
    "drums",
    "figuremode",
    "figures",
    "header",
    "in",
    "include",
    "language",
    "layout",
    "longa",
    "lyricmode",
    "lyrics",
    "lyricsto",
    "markup",
    "markuplist",
    "maxima",
    "midi",
    "mm",
    "name",
    "new",
    "notemode",
    "override",
    "paper",
    "pt",
    "remove",
    "repeat",
    "revert",
    "score",
    "sequential",
    "set",
    "simultaneous",
    "tempo",
    "type",
    "unset",
    "version",
    "with",
    "default",
    "etc",
    "rest",
    "maininput",
    "sourcefilename",
    "sourcefileline",
    "inherit-acceptability",
    "score-lines",
    "version-error",
];

/// Which names a note is checked against.
#[derive(Clone, Copy, PartialEq)]
enum Table {
    Pitches,
    Drums,
    /// Chord, lyric and figure modes, markup, and the output blocks, where words are not notes.
    Off,
}

#[derive(Clone)]
struct State {
    languages: Languages,
    table: Table,
    in_markup: bool,
}

/// Every unknown command and invalid note name in `doc`. Nothing is reported until LilyPond's
/// command data has loaded, since until then every name is unknown.
pub fn name_checks(
    doc: &Document,
    data: Option<&CommandData>,
    index: &Index,
    enc: Encoding,
) -> Vec<Diagnostic> {
    let Some(data) = data else { return Vec::new() };
    let path = file_uri::to_path(&doc.uri);
    let own = summarize(
        &doc.tree,
        &doc.text,
        path.as_deref().unwrap_or(Path::new("")),
    );
    let visible = path
        .as_deref()
        .map(|p| index.definitions_visible_from(p))
        .unwrap_or_default();
    let mut defined = HashSet::new();
    let mut functions = HashMap::new();
    for definition in own.definitions.iter().chain(&visible) {
        defined.insert(definition.name.as_str());
        if let Some(signature) = &definition.signature {
            functions
                .entry(definition.name.clone())
                .or_insert_with(|| signature.clone());
        }
    }
    let languages = match &path {
        Some(path) => index.start_languages(path),
        None => Some(BTreeSet::from([DEFAULT_LANGUAGE.to_owned()])),
    };
    let mut checker = Checker {
        doc,
        data,
        index,
        enc,
        path,
        defined,
        signatures: Signatures {
            data,
            own: &functions,
        },
        found: Vec::new(),
    };
    let mut state = State {
        languages,
        table: Table::Pitches,
        in_markup: false,
    };
    checker.walk(doc.tree.root_node(), &mut state);
    checker.found
}

struct Checker<'a> {
    doc: &'a Document,
    data: &'a CommandData,
    index: &'a Index,
    enc: Encoding,
    path: Option<PathBuf>,
    defined: HashSet<&'a str>,
    signatures: Signatures<'a>,
    found: Vec<Diagnostic>,
}

impl Checker<'_> {
    /// Checks what `parent` holds. A `\language` changes `state` for everything after it, out
    /// of plain braces too, as it does in LilyPond, while a mode or block keeps its changes to
    /// itself.
    fn walk(&mut self, parent: Node, state: &mut State) {
        let text = &self.doc.text;
        let mut cursor = parent.walk();
        let children: Vec<Node> = parent.named_children(&mut cursor).collect();
        // A word a function takes as a name, such as the `treble` in `\clef treble`, is not a
        // note.
        let mut not_notes = HashSet::new();
        for (i, child) in children.iter().enumerate() {
            if child.kind() == "command" {
                let name = node_text::command(*child, text);
                for (node, kind) in bind_with(&name, &children[i + 1..], text, &self.signatures) {
                    if matches!(kind, ArgKind::Word | ArgKind::Markup | ArgKind::Scheme) {
                        not_notes.insert(node.id());
                    }
                }
            }
        }
        for (i, &child) in children.iter().enumerate() {
            match child.kind() {
                "language" => {
                    if let Some(name) = child.child_by_field_name("name") {
                        set_language(state, node_text::string(name, text));
                    }
                }
                "include" => {
                    if let Some(file) = child.child_by_field_name("path") {
                        self.include(state, &node_text::string(file, text));
                    }
                }
                "command" => {
                    let name = node_text::command(child, text);
                    let argument = children[i + 1..]
                        .iter()
                        .find(|n| !n.is_extra())
                        .filter(|n| n.kind() == "string");
                    match (name.as_str(), argument) {
                        ("language", Some(&language)) => {
                            set_language(state, node_text::string(language, text))
                        }
                        ("include", Some(&file)) => {
                            self.include(state, &node_text::string(file, text))
                        }
                        _ => {}
                    }
                    self.check_command(child, &name, state.in_markup);
                }
                "note" if !not_notes.contains(&child.id()) => {
                    self.check_note(child, state);
                    self.walk(child, state);
                }
                kind => match scoped(kind, state) {
                    Some(mut inner) => self.walk(child, &mut inner),
                    None => self.walk(child, state),
                },
            }
        }
    }

    fn include(&self, state: &mut State, file: &str) {
        if state.table == Table::Off {
            return;
        }
        if let Some(path) = &self.path {
            state.languages =
                self.index
                    .languages_after_include(path, file, state.languages.take());
        }
    }

    fn check_command(&mut self, node: Node, name: &str, in_markup: bool) {
        if KEYWORDS.contains(&name)
            || self.data.is_known_command(name, in_markup)
            || self.defined.contains(name)
        {
            return;
        }
        let message = format!(
            "unknown command `{}`",
            node_text::text(node, &self.doc.text)
        );
        self.found.push(error(
            self.doc,
            self.enc,
            node.start_byte(),
            node.end_byte(),
            message,
        ));
    }

    fn check_note(&mut self, note: Node, state: &State) {
        let Some(word) = note
            .child_by_field_name("pitch")
            .and_then(|p| p.named_child(0))
            .filter(|n| n.kind() == "note_name")
        else {
            return;
        };
        let name = node_text::text(word, &self.doc.text);
        let message = match state.table {
            Table::Off => return,
            Table::Drums => {
                let drums = self.data.drums();
                if drums.is_empty() || drums.contains(name) {
                    return;
                }
                format!("`{name}` is not a drum name")
            }
            Table::Pitches => {
                let Some(languages) = &state.languages else {
                    return;
                };
                let mut tables = Vec::new();
                for language in languages {
                    // A language the data does not know, or knows without pitches, is not
                    // checked.
                    match self.data.pitches(language) {
                        Some(table) if !table.is_empty() => tables.push(table),
                        _ => return,
                    }
                }
                if tables.is_empty() || tables.iter().any(|t| t.contains(name)) {
                    return;
                }
                let languages: Vec<&str> = languages.iter().map(String::as_str).collect();
                format!("`{name}` is not a note name in {}", languages.join(" or "))
            }
        };
        self.found.push(error(
            self.doc,
            self.enc,
            word.start_byte(),
            word.end_byte(),
            message,
        ));
    }
}

/// A `\language` in note or drum mode switches to that language's pitches until the mode ends.
/// Elsewhere it changes nothing this check uses.
fn set_language(state: &mut State, name: String) {
    if state.table != Table::Off {
        state.languages = Some(BTreeSet::from([name]));
        state.table = Table::Pitches;
    }
}

/// The state inside a node that keeps its own, or `None` for one that shares its parent's.
fn scoped(kind: &str, state: &State) -> Option<State> {
    let with = |table, in_markup| {
        Some(State {
            languages: state.languages.clone(),
            table,
            in_markup,
        })
    };
    match kind {
        "note_mode" => with(Table::Pitches, false),
        "drum_mode" => with(Table::Drums, false),
        "chord_mode" | "lyric_mode" | "lyrics_to" | "add_lyrics" | "figure_mode"
        | "paper_block" | "layout_block" | "midi_block" => with(Table::Off, false),
        "markup" | "markup_list" => with(Table::Off, true),
        "score_block" | "embedded_lilypond" if state.in_markup => with(Table::Pitches, false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::testing::{Scratch, document, mini};

    /// The messages `name_checks` gives for an untitled document holding `source`.
    fn messages(source: &str) -> Vec<String> {
        let index = Index::new(Vec::new());
        checked(&document(None, source), &index)
    }

    fn checked(doc: &Document, index: &Index) -> Vec<String> {
        let data = mini();
        name_checks(doc, Some(&data), index, Encoding::Utf8)
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    fn open(index: &mut Index, path: &Path, source: &str) -> Document {
        let doc = document(Some(path), source);
        index.update(path, &doc);
        doc
    }

    #[test]
    fn unknown_command_flagged() {
        let index = Index::new(Vec::new());
        let doc = document(None, "{ \\melodyy }");
        let data = mini();
        let found = name_checks(&doc, Some(&data), &index, Encoding::Utf8);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].message, "unknown command `\\melodyy`");
        assert_eq!(
            found[0].range,
            lsp_types::Range::new(
                lsp_types::Position::new(0, 2),
                lsp_types::Position::new(0, 10)
            )
        );
    }

    #[test]
    fn defined_variable_not_flagged() {
        assert_eq!(
            messages("melody = { c }\n{ \\melody \\melodyy }"),
            ["unknown command `\\melodyy`"]
        );
    }

    #[test]
    fn variable_from_include_not_flagged() {
        let dir = Scratch::new("names-include");
        dir.write("defs.ily", "tune = { c }\n");
        let main = dir.path().join("main.ly");
        let mut index = Index::new(Vec::new());
        let doc = open(
            &mut index,
            &main,
            "\\include \"defs.ily\"\n{ \\tune \\tunee }\n",
        );
        assert_eq!(checked(&doc, &index), ["unknown command `\\tunee`"]);
    }

    #[test]
    fn clef_word_not_a_pitch() {
        assert_eq!(messages("{ \\clef treble c }"), Vec::<String>::new());
    }

    #[test]
    fn invalid_note_flagged() {
        assert_eq!(
            messages("{ xyz }"),
            ["`xyz` is not a note name in nederlands"]
        );
    }

    #[test]
    fn english_pitch_rules() {
        let source = "\\language \"english\"\n{ cs cis }\n\\notemode { \\language \"nederlands\" cis }\n{ cis }";
        assert_eq!(
            messages(source),
            [
                "`cis` is not a note name in english",
                "`cis` is not a note name in english"
            ]
        );
    }

    #[test]
    fn chordmode_ignores_language() {
        assert_eq!(
            messages(
                "\\language \"english\"\n\\chordmode { cis:m \\language \"nederlands\" }\n{ cs }"
            ),
            Vec::<String>::new()
        );
    }

    #[test]
    fn drum_names_checked() {
        assert_eq!(
            messages("\\drummode { bd sn xx }"),
            ["`xx` is not a drum name"]
        );
    }

    #[test]
    fn nothing_before_data_loads() {
        let index = Index::new(Vec::new());
        let doc = document(None, "{ \\melodyy xyz }");
        assert!(name_checks(&doc, None, &index, Encoding::Utf8).is_empty());
    }

    #[test]
    fn markup_commands_checked_in_markup() {
        assert_eq!(
            messages("\\markup { \\bold \\column { a } \\frob }\n{ \\bold }"),
            ["unknown command `\\frob`", "unknown command `\\bold`"]
        );
    }

    #[test]
    fn keywords_not_flagged() {
        let source = "\\paper { indent = 3\\mm }\n\\markup { \\override #'(baseline-skip . 2) \\bold x }\n{ \\key d \\default }";
        assert_eq!(messages(source), Vec::<String>::new());
    }

    #[test]
    fn user_function_args_bind() {
        let source =
            "fn = #(define-music-function (sym m) (symbol? ly:music?) m)\n{ \\fn treble { c } }";
        assert_eq!(messages(source), Vec::<String>::new());
    }

    #[test]
    fn language_from_include_applies() {
        let dir = Scratch::new("names-language-include");
        dir.write("share/ly/english.ly", "\\language \"english\"\n");
        let main = dir.path().join("main.ly");
        let mut index = Index::new(Vec::new());
        index.set_datadir(Some(dir.path().join("share")));
        let doc = open(&mut index, &main, "\\include \"english.ly\"\n{ cs cis }\n");
        assert_eq!(
            checked(&doc, &index),
            ["`cis` is not a note name in english"]
        );
    }

    #[test]
    fn ily_takes_language_from_includer() {
        let dir = Scratch::new("names-ily-language");
        let main = dir.path().join("main.ly");
        let part = dir.path().join("part.ily");
        let mut index = Index::new(Vec::new());
        let part_doc = open(&mut index, &part, "{ cs cis }\n");
        assert_eq!(checked(&part_doc, &index), Vec::<String>::new());
        open(
            &mut index,
            &main,
            "\\language \"english\"\n\\include \"part.ily\"\n",
        );
        assert_eq!(
            checked(&part_doc, &index),
            ["`cis` is not a note name in english"]
        );
    }
}
