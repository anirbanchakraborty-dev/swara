//! The protocol basics: position encodings, capabilities, syntax diagnostics and incremental
//! parsing.

mod client;

use std::fmt::Write as _;
use std::time::Duration;

use client::{TestClient, uri};
use lsp_types::request::{DocumentSymbolRequest, GotoDefinition};
use lsp_types::{
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    Position, PositionEncodingKind, Range, TextDocumentIdentifier, TextDocumentPositionParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncSaveOptions,
};
use serde_json::{Value, json};
use tree_sitter::{Node, Parser};

const WAIT: Duration = Duration::from_secs(5);

fn options() -> Value {
    json!({"autoCheck": false, "lilypond": null, "includePaths": []})
}

#[test]
fn negotiates_utf8() {
    let both = TestClient::spawn_offering(options(), &["utf-8", "utf-16"]);
    let encoding = &both.initialize_result.capabilities.position_encoding;
    assert_eq!(encoding.as_ref(), Some(&PositionEncodingKind::UTF8));
    let utf16 = TestClient::spawn_offering(options(), &["utf-16"]);
    let encoding = &utf16.initialize_result.capabilities.position_encoding;
    assert_eq!(encoding.as_ref(), Some(&PositionEncodingKind::UTF16));
}

#[test]
fn advertises_capabilities() {
    let client = TestClient::spawn(options());
    let caps = &client.initialize_result.capabilities;
    let Some(TextDocumentSyncCapability::Options(sync)) = &caps.text_document_sync else {
        panic!("no sync options: {:?}", caps.text_document_sync);
    };
    assert_eq!(sync.open_close, Some(true));
    assert_eq!(sync.change, Some(TextDocumentSyncKind::INCREMENTAL));
    assert!(matches!(sync.save, Some(TextDocumentSyncSaveOptions::SaveOptions(_))));
    let triggers = caps.completion_provider.as_ref().unwrap().trigger_characters.clone();
    assert_eq!(triggers, Some(vec!["\\".to_string(), ".".to_string()]));
    assert!(caps.hover_provider.is_some());
    assert!(caps.definition_provider.is_some());
    assert!(caps.document_symbol_provider.is_some());
    assert!(caps.document_formatting_provider.is_some());
    let commands = &caps.execute_command_provider.as_ref().unwrap().commands;
    assert_eq!(commands, &["swara.build", "swara.preview"]);
}

#[test]
fn syntax_error_published() {
    let mut client = TestClient::spawn(options());
    let file = uri("/music/lesson.ly");
    client.open(&file, "{ c d");
    let (diagnostics, _) = client.wait_diagnostics(&file, WAIT, |d| !d.is_empty());
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].message, "missing `}`");
    assert_eq!(diagnostics[0].source.as_deref(), Some("swara"));
    assert_eq!(diagnostics[0].range.start, Position::new(0, 5));
}

#[test]
fn unexpected_token_published() {
    let mut client = TestClient::spawn(options());
    let file = uri("/music/lesson.ly");
    client.open(&file, "{ c d } }");
    let (diagnostics, _) = client.wait_diagnostics(&file, WAIT, |d| !d.is_empty());
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].message, "unexpected `}`");
    assert_eq!(diagnostics[0].range, Range::new(Position::new(0, 8), Position::new(0, 9)));
}

#[test]
fn edit_fixes_error() {
    let mut client = TestClient::spawn(options());
    let file = uri("/music/lesson.ly");
    client.open(&file, "{ c d");
    client.wait_diagnostics(&file, WAIT, |d| !d.is_empty());
    let end = Position::new(0, 5);
    client.change(&file, 2, Range::new(end, end), " }");
    let (diagnostics, _) = client.wait_diagnostics(&file, WAIT, |d| d.is_empty());
    assert!(diagnostics.is_empty());
}

#[test]
fn utf16_client_gets_utf16_ranges() {
    let mut client = TestClient::spawn_offering(options(), &["utf-16"]);
    let file = uri("/music/আমার গান/lesson 1.ly");
    client.open(&file, "{ c^\"𝄞\" } }");
    let (diagnostics, _) = client.wait_diagnostics(&file, WAIT, |d| !d.is_empty());
    // 𝄞 is two UTF-16 units, so the stray brace starts at unit 11, though at byte 13.
    assert_eq!(diagnostics[0].range.start, Position::new(0, 11), "{diagnostics:?}");
}

#[test]
fn definition_and_outline_in_utf16() {
    let mut client = TestClient::spawn_offering(options(), &["utf-16"]);
    let defs = uri("/music/defs.ily");
    let main = uri("/music/main.ly");
    // Each Bengali letter is three bytes and one UTF-16 unit, so the name ends at unit 5.
    client.open(&defs, "\"সুর\" = { c }\n");
    client.open(&main, "\\include \"defs.ily\"\n{ \\\"সুর\" }\n");
    let found = client.request::<GotoDefinition>(GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: main.clone() },
            position: Position::new(1, 5),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    });
    let Some(GotoDefinitionResponse::Scalar(location)) = found else {
        panic!("no definition: {found:?}");
    };
    assert_eq!(location.uri, defs);
    assert_eq!(location.range, Range::new(Position::new(0, 0), Position::new(0, 5)));
    let outline = client.request::<DocumentSymbolRequest>(DocumentSymbolParams {
        text_document: TextDocumentIdentifier { uri: defs.clone() },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    });
    let Some(DocumentSymbolResponse::Nested(symbols)) = outline else {
        panic!("no outline: {outline:?}");
    };
    assert_eq!(symbols[0].name, "সুর");
    assert_eq!(symbols[0].selection_range.end, Position::new(0, 5));
}

/// A small xorshift generator, so the edits are the same on every run.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

fn position(text: &str, byte: usize, utf16: bool) -> Position {
    let before = &text[..byte];
    let line_text = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
    let character = if utf16 { line_text.encode_utf16().count() } else { line_text.len() };
    Position::new(before.matches('\n').count() as u32, character as u32)
}

/// Every node, named or not, with its kind and byte range. `swara/debugTree` writes the same.
fn dump(node: Node, out: &mut String) {
    let missing = if node.is_missing() { " MISSING" } else { "" };
    write!(out, "({}{missing} {}-{}", node.kind(), node.start_byte(), node.end_byte()).unwrap();
    for child in node.children(&mut node.walk()) {
        out.push(' ');
        dump(child, out);
    }
    out.push(')');
}

fn fresh_dump(text: &str) -> String {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_lilypond::LANGUAGE.into()).unwrap();
    let tree = parser.parse(text, None).unwrap();
    let mut out = String::new();
    dump(tree.root_node(), &mut out);
    out
}

fn random_edits(encodings: &[&'static str], utf16: bool) {
    let mut client = TestClient::spawn_offering(options(), encodings);
    let file = uri("/music/edits.ly");
    let mut text = String::from(
        "\\version \"2.26.0\"\n\\relative c' {\n  c4 d e f | g2 g\n  \\new Staff = \"গান\" { a8( b) c'4^\"𝄞\" }\n}\n",
    );
    client.open(&file, &text);
    let alphabet = [
        "{", "}", "<", ">", "\\", "c", "d", "4", "'", ",", "\"", "#", "(", ")", " ", "\n", "|",
        "=", "গ", "𝄞", "%",
    ];
    let mut rng = Rng(0x5eed_1234_abcd);
    for version in 2..202 {
        let starts: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
        let at = starts[rng.below(starts.len())];
        let (end, insert) = if at < text.len() && rng.below(3) == 0 {
            (at + text[at..].chars().next().unwrap().len_utf8(), "")
        } else {
            (at, alphabet[rng.below(alphabet.len())])
        };
        let range = Range::new(position(&text, at, utf16), position(&text, end, utf16));
        client.change(&file, version, range, insert);
        text.replace_range(at..end, insert);
    }
    let result = client.request_raw("swara/debugTree", TextDocumentIdentifier::new(file));
    let result = result.response_result.expect("swara/debugTree answers when SWARA_TEST=1");
    assert_eq!(result["text"], text);
    assert_eq!(result["tree"], fresh_dump(&text));
}

#[test]
fn incremental_equals_fresh() {
    random_edits(&["utf-8", "utf-16"], false);
    random_edits(&["utf-16"], true);
}
