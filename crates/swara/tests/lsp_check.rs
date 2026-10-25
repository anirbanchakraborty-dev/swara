//! The LilyPond check after each pause in typing: when it runs, what it publishes, and that it
//! never leaves a process behind. A fake `lilypond` stands in for the real one, except in the one
//! test marked `#[ignore]`.

// The fake is a shell script, and the process checks use Unix signals.
#![cfg(unix)]

mod client;

use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, SystemTime};

use client::fake::{alive, assert_all_gone, start, wait_until};
use client::{TestClient, uri};
use lsp_types::{DiagnosticSeverity, Position, Range, Uri};
use serde_json::json;

const WAIT: Duration = Duration::from_secs(5);

fn fixture(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/lilypond");
    fs::canonicalize(dir.join(name)).unwrap()
}

fn scratch(name: &str) -> PathBuf {
    client::scratch("lsp_check", name)
}

fn file_uri(path: &Path) -> Uri {
    uri(path.to_str().unwrap())
}

fn insert(client: &mut TestClient, file: &Uri, version: i32, at: Position, text: &str) {
    client.change(file, version, Range::new(at, at), text);
}

fn from_lilypond(diagnostic: &&lsp_types::Diagnostic) -> bool {
    diagnostic.source.as_deref() == Some("lilypond")
}

#[test]
fn open_never_runs_lilypond() {
    let dir = scratch("open");
    let (mut client, fake) = start(&dir, "", true, &[]);
    client.open(&file_uri(&dir.join("lesson.ly")), "{ c'4 }\n");
    sleep(Duration::from_secs(1));
    assert!(fake.runs().is_empty(), "{:?}", fake.runs());
}

#[test]
fn one_check_after_typing_stops() {
    let dir = scratch("typing");
    let (mut client, fake) = start(&dir, "sleep:0", true, &[]);
    let file = file_uri(&dir.join("lesson.ly"));
    client.open(&file, "{ c'4 }\n");
    let mut last_edit = SystemTime::now();
    for version in 2..52 {
        last_edit = SystemTime::now();
        insert(&mut client, &file, version, Position::new(0, 2), "d ");
        sleep(Duration::from_millis(20));
    }
    assert!(wait_until(WAIT, || !fake.runs().is_empty()), "no check ran");
    let started = fs::metadata(&fake.log).unwrap().modified().unwrap();
    let after = started.duration_since(last_edit).unwrap_or_default();
    assert!(
        after >= Duration::from_millis(330),
        "the check started {after:?} after the last edit"
    );
    assert_all_gone(&fake, 2);
    sleep(Duration::from_millis(500));
    assert_eq!(fake.runs().len(), 1, "{:?}", fake.runs());
}

#[test]
fn stale_check_killed() {
    let dir = scratch("stale");
    let mode = format!("stderr:{},sleep:5", fixture("parse-error.stderr").display());
    let (mut client, fake) = start(&dir, &mode, true, &[]);
    let file = file_uri(&dir.join("lesson.ly"));
    client.open(&file, "{ c'4 }\n");
    insert(&mut client, &file, 2, Position::new(0, 2), "d ");
    assert!(
        wait_until(WAIT, || fake.pids().len() == 2),
        "the first check never started"
    );
    let first = fake.pids();
    sleep(Duration::from_millis(100));
    insert(&mut client, &file, 3, Position::new(0, 2), "e ");
    // The first run printed its error before it was stopped, so publishing its result would show.
    let publishes = client.publishes_for(&file, Duration::from_millis(1500));
    assert!(
        publishes.iter().flatten().all(|d| !from_lilypond(&d)),
        "{publishes:?}"
    );
    let gone = wait_until(Duration::from_secs(3), || {
        first.iter().all(|&pid| !alive(pid))
    });
    assert!(gone, "the first check is still running: {first:?}");
    assert_eq!(fake.runs().len(), 2, "{:?}", fake.runs());
    // Shutting the server down stops the second.
    drop(client);
    assert_all_gone(&fake, 4);
}

#[test]
fn runaway_check_times_out() {
    let dir = scratch("runaway");
    let env = [("SWARA_TEST_CHECK_TIMEOUT_MS", "1000")];
    let (mut client, fake) = start(&dir, "sleep:60", true, &env);
    let file = file_uri(&dir.join("lesson.ly"));
    client.open(&file, "{ c'4 }\n");
    insert(&mut client, &file, 2, Position::new(0, 2), "d ");
    let information = Some(DiagnosticSeverity::INFORMATION);
    let (diagnostics, _) =
        client.wait_diagnostics(&file, WAIT, |d| d.iter().any(|d| d.severity == information));
    let stopped: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == information)
        .collect();
    assert_eq!(stopped.len(), 1, "{diagnostics:?}");
    assert_eq!(
        stopped[0].message,
        "LilyPond check took longer than 1 s and was stopped"
    );
    assert_eq!(stopped[0].source.as_deref(), Some("lilypond"));
    assert_all_gone(&fake, 2);
}

#[test]
fn check_errors_published() {
    let dir = scratch("errors");
    let mode = format!("stderr:{}", fixture("parse-error.stderr").display());
    let (mut client, _fake) = start(&dir, &mode, true, &[]);
    let file = file_uri(&dir.join("parse-error.ly"));
    client.open(
        &file,
        &fs::read_to_string(fixture("parse-error.ly")).unwrap(),
    );
    // A comment typed on a new last line starts the check without moving line 2.
    insert(&mut client, &file, 2, Position::new(2, 0), "% done\n");
    let (diagnostics, _) =
        client.wait_diagnostics(&file, WAIT, |d| d.iter().any(|d| from_lilypond(&d)));
    let lilypond: Vec<_> = diagnostics.iter().filter(from_lilypond).collect();
    assert_eq!(lilypond.len(), 1, "{diagnostics:?}");
    assert_eq!(lilypond[0].message, "syntax error, unexpected '}'");
    assert_eq!(lilypond[0].severity, Some(DiagnosticSeverity::ERROR));
    assert_eq!(
        lilypond[0].range,
        Range::new(Position::new(1, 11), Position::new(1, 12))
    );
    // The stray brace is a syntax error in the tree too, so the instant layer still shows it.
    let swara = diagnostics
        .iter()
        .filter(|d| d.source.as_deref() == Some("swara"))
        .count();
    assert_eq!(swara, 1, "{diagnostics:?}");
}

#[test]
fn ily_gets_its_own_diagnostics() {
    let dir = scratch("ily").join("আমার গান");
    fs::create_dir_all(&dir).unwrap();
    let part = dir.join("part 1.ily");
    fs::write(&part, "partMusic = {\n  c'4 d' e'\n}\n").unwrap();
    let stderr = dir.join("stderr.txt");
    let message = format!(
        "{}:2:7: error: unknown command: `\\fooBar'\n",
        part.display()
    );
    fs::write(&stderr, message).unwrap();
    let (mut client, _fake) = start(&dir, &format!("stderr:{}", stderr.display()), true, &[]);
    let main = file_uri(&dir.join("main.ly"));
    client.open(&main, "\\include \"part 1.ily\"\n{ \\partMusic }\n");
    insert(&mut client, &main, 2, Position::new(2, 0), "% done\n");
    let part = file_uri(&part);
    let (diagnostics, _) = client.wait_diagnostics(&part, WAIT, |d| !d.is_empty());
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].source.as_deref(), Some("lilypond"));
    assert_eq!(
        diagnostics[0].range,
        Range::new(Position::new(1, 6), Position::new(1, 8))
    );
    // The next check is clean, so the include's diagnostics clear.
    fs::write(&stderr, "").unwrap();
    insert(&mut client, &main, 3, Position::new(3, 0), "% again\n");
    client.wait_diagnostics(&part, WAIT, <[_]>::is_empty);
}

#[test]
fn auto_check_off() {
    let dir = scratch("off");
    let (mut client, fake) = start(&dir, "", false, &[]);
    let file = file_uri(&dir.join("lesson.ly"));
    client.open(&file, "{ c'4 }\n");
    insert(&mut client, &file, 2, Position::new(0, 2), "d ");
    insert(&mut client, &file, 3, Position::new(0, 2), "e ");
    sleep(Duration::from_secs(1));
    assert!(fake.runs().is_empty(), "{:?}", fake.runs());
}

#[test]
#[ignore = "runs the real LilyPond"]
fn real_check_finds_seeded_errors() {
    let path = fixture("seeded.ly");
    let text = fs::read_to_string(&path).unwrap();
    let mut client = TestClient::spawn(json!({"autoCheck": true, "lilypond": null}));
    let file = file_uri(&path);
    client.open(&file, &text);
    let end = Position::new(text.lines().count() as u32, 0);
    insert(&mut client, &file, 2, end, "% done\n");
    let enough = |d: &[lsp_types::Diagnostic]| d.iter().filter(from_lilypond).count() >= 4;
    let (diagnostics, _) = client.wait_diagnostics(&file, Duration::from_secs(20), enough);
    for expected in fs::read_to_string(fixture("seeded.expect"))
        .unwrap()
        .lines()
    {
        let [line, severity, words] = expected.splitn(3, ':').collect::<Vec<_>>()[..] else {
            panic!("bad line in seeded.expect: {expected}");
        };
        // The tablature warning comes only from engraving, which the check skips.
        if words.starts_with("No string for pitch") {
            continue;
        }
        let line: u32 = line.parse().unwrap();
        let severity = match severity {
            "error" => DiagnosticSeverity::ERROR,
            "warning" => DiagnosticSeverity::WARNING,
            other => panic!("unknown severity {other}"),
        };
        let found = diagnostics.iter().filter(from_lilypond).any(|d| {
            d.range.start.line == line - 1
                && d.severity == Some(severity)
                && d.message.contains(words)
        });
        assert!(found, "no {expected} in {diagnostics:?}");
    }
}

#[test]
#[ignore = "runs the real LilyPond"]
fn real_check_reads_includes_from_a_bengali_folder() {
    // The check passes the buffer's folder with `-I`, so an include beside a file in a Bengali
    // folder with a space in its name must still be found and its errors placed on its lines.
    let dir = scratch("real-bengali").join("আমার গান");
    fs::create_dir_all(&dir).unwrap();
    let part = dir.join("part 1.ily");
    fs::write(&part, "partMusic = { c'4 \\fooBar d' }\n").unwrap();
    let mut client = TestClient::spawn(json!({"autoCheck": true, "lilypond": null}));
    let main = file_uri(&dir.join("main.ly"));
    client.open(
        &main,
        "\\version \"2.26.0\"\n\\include \"part 1.ily\"\n{ \\partMusic }\n",
    );
    insert(&mut client, &main, 2, Position::new(3, 0), "% done\n");
    let unknown = |d: &[lsp_types::Diagnostic]| d.iter().any(|d| d.message.contains("fooBar"));
    let (diagnostics, _) =
        client.wait_diagnostics(&file_uri(&part), Duration::from_secs(20), unknown);
    assert_eq!(
        diagnostics[0].range.start,
        Position::new(0, 18),
        "{diagnostics:?}"
    );
}
