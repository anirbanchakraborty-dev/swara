//! `swara format` on the command line, and formatting over the protocol.

mod client;

use std::io::Write as _;
use std::process::{Command, Stdio};

use client::{TestClient, uri};
use lsp_types::request::Formatting;
use lsp_types::{
    DocumentFormattingParams, FormattingOptions, Position, Range, TextDocumentIdentifier, TextEdit,
    WorkDoneProgressParams,
};
use serde_json::json;

/// Runs `swara` with `args`, feeding it `input`, and gives back its output, errors and exit code.
fn run(args: &[&str], input: &str) -> (String, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_swara"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("swara starts");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
        output.status.code().unwrap_or(-1),
    )
}

#[test]
fn cli_formats_stdin() {
    let (out, err, code) = run(&["format", "--indent", "4"], "{\nc  d\n}\n");
    assert_eq!(
        (out.as_str(), err.as_str(), code),
        ("{\n    c d\n}\n", "", 0)
    );
    let (out, _, code) = run(&["format"], "{\nc  d\n}\n");
    assert_eq!((out.as_str(), code), ("{\n  c d\n}\n", 0));
    let (out, _, code) = run(&["format"], "{ c d }\n");
    assert_eq!((out.as_str(), code), ("{ c d }\n", 0));
}

#[test]
fn cli_refusal_passes_input_through() {
    let (out, err, code) = run(&["format", "--indent", "2"], "{  c");
    assert_eq!(out, "{  c");
    assert!(err.contains("syntax errors"), "{err}");
    assert_eq!(code, 0);
}

#[test]
fn cli_rejects_a_bad_indent() {
    let (out, err, code) = run(&["format", "--indent", "two"], "{ c }\n");
    assert_eq!((out.as_str(), code), ("", 2));
    assert!(err.contains("usage"), "{err}");
}

#[test]
fn formatting_edits_changed_lines_in_utf16() {
    let mut client = TestClient::spawn_offering(
        json!({"autoCheck": false, "lilypond": null, "includePaths": []}),
        &["utf-16"],
    );
    let song = uri("/music/song.ly");
    client.open(&song, "\\header {\ntitle = \"সুর\"   \n}\n");
    let params = |song: &lsp_types::Uri| DocumentFormattingParams {
        text_document: TextDocumentIdentifier { uri: song.clone() },
        options: FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            ..Default::default()
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let edits = client.request::<Formatting>(params(&song));
    assert_eq!(
        edits,
        Some(vec![TextEdit::new(
            Range::new(Position::new(1, 0), Position::new(1, 16)),
            "    title = \"সুর\"".to_owned()
        )])
    );
    let broken = uri("/music/broken.ly");
    client.open(&broken, "{  c\n");
    assert_eq!(client.request::<Formatting>(params(&broken)), None);
}
