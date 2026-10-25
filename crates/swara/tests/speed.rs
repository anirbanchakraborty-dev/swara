//! The server against the design's speed targets. Each test runs the real LilyPond where it needs
//! one, and reports the median of 20 runs taken after one warm-up run, with the fastest and the
//! slowest, so that a miss can be recorded. The targets are for a release build, so this file is
//! compiled only into one:
//!
//! `cargo test -p swara --release --test speed -- --include-ignored --nocapture`

#![cfg(not(debug_assertions))]

mod client;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::thread::sleep;
use std::time::{Duration, Instant};

use client::{TestClient, uri};
use lsp_types::{Diagnostic, Position, Range, Uri};
use serde_json::{Value, json};

const RUNS: usize = 20;
const WAIT: Duration = Duration::from_secs(30);
const FINISHED: &str = "swara/buildFinished";
const SONG: &str = include_str!("../../../bench/song-100.ly");

/// Held by each test for its whole run. The test harness runs tests side by side, and a build
/// running beside a timing would measure the build.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner)
}

fn scratch(name: &str) -> PathBuf {
    client::scratch("speed", name)
}

fn file_uri(path: &Path) -> Uri {
    uri(path.to_str().unwrap())
}

fn real_client(auto_check: bool) -> TestClient {
    TestClient::spawn(json!({"autoCheck": auto_check, "lilypond": null}))
}

/// Waits until the server has LilyPond's command data, dumping it into the cache on the first
/// start and reading it from there afterwards.
fn wait_for_data(client: &mut TestClient) {
    let deadline = Instant::now() + WAIT;
    loop {
        let reply = client.request_raw("swara/debugCommandData", json!({}));
        if !reply.response_result.unwrap_or(Value::Null).is_null() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "command data did not load within {WAIT:?}"
        );
        sleep(Duration::from_millis(20));
    }
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Runs `run` once as a warm-up, then `RUNS` times, prints the timings and returns their median.
fn median_of(name: &str, mut run: impl FnMut() -> Duration) -> Duration {
    run();
    let mut times: Vec<Duration> = (0..RUNS).map(|_| run()).collect();
    times.sort();
    let median = (times[RUNS / 2 - 1] + times[RUNS / 2]) / 2;
    let (fastest, slowest) = (times[0], times[RUNS - 1]);
    println!(
        "{name}: median {:.1} ms, fastest {:.1} ms, slowest {:.1} ms",
        ms(median),
        ms(fastest),
        ms(slowest)
    );
    median
}

fn assert_under(name: &str, median: Duration, target: Duration) {
    assert!(
        median < target,
        "{name}: median {:.1} ms against a target of {:.0} ms",
        ms(median),
        ms(target)
    );
}

/// The song cut to its first four bars and four lyric lines: the 4-bar lesson, which is what
/// `cargo xtask gen-song --bars 4` writes.
fn lesson() -> String {
    let (mut bars, mut words) = (0, 0);
    let keep = |line: &&str| {
        if line.ends_with(" |") {
            bars += 1;
            bars <= 4
        } else if line.starts_with("  la ") {
            words += 1;
            words <= 4
        } else {
            true
        }
    };
    SONG.lines()
        .filter(keep)
        .map(|line| format!("{line}\n"))
        .collect()
}

fn from_lilypond(diagnostic: &Diagnostic) -> bool {
    diagnostic.source.as_deref() == Some("lilypond")
}

#[test]
#[ignore = "a timing test with the real LilyPond"]
fn ready_under_100ms() {
    let _serial = one_at_a_time();
    // The first start fills the cache when it is empty, so every timed start finds it.
    wait_for_data(&mut real_client(false));
    let median = median_of("ready", || {
        let start = Instant::now();
        let client = real_client(false);
        let ready = start.elapsed();
        drop(client);
        ready
    });
    assert_under("ready", median, Duration::from_millis(100));
}

#[test]
#[ignore = "a timing test with the real LilyPond"]
fn instant_under_50ms() {
    let _serial = one_at_a_time();
    let path = scratch("instant").join("song.ly");
    fs::write(&path, SONG).unwrap();
    let mut client = real_client(false);
    wait_for_data(&mut client);
    let file = file_uri(&path);
    client.open(&file, SONG);
    let (opened, _) = client.wait_diagnostics(&file, WAIT, |_| true);
    assert!(opened.is_empty(), "{opened:?}");
    // The first note of the first bar, `c'8` on the sixth line, typed over with a closing brace
    // and back, so that each publish shows by its errors which text it checked.
    let note = Range::new(Position::new(5, 2), Position::new(5, 3));
    let mut version = 1;
    let median = median_of("instant checks", || {
        version += 1;
        let broken = version % 2 == 0;
        client.change(&file, version, note, if broken { "}" } else { "c" });
        client
            .wait_diagnostics(&file, WAIT, |d| d.is_empty() != broken)
            .1
    });
    assert_under("instant checks", median, Duration::from_millis(50));
}

#[test]
#[ignore = "a timing test with the real LilyPond"]
fn check_under_600ms() {
    let _serial = one_at_a_time();
    // Only LilyPond reports this warning, and each edit changes its digit, so the publish that
    // carries the new digit is the check of the new text. An edit moves the old check's messages
    // rather than removing them, so without the digit the instant publish would already match.
    let mark = "#(ly:warning \"speed 0\")";
    let text = SONG.replacen("\n\nmus = {", &format!("\n{mark}\n\nmus = {{"), 1);
    let line = text.lines().position(|l| l == mark).unwrap() as u32;
    let column = mark.find('0').unwrap() as u32;
    let digit_at = Range::new(Position::new(line, column), Position::new(line, column + 1));
    let path = scratch("check").join("song.ly");
    fs::write(&path, &text).unwrap();
    let mut client = real_client(true);
    wait_for_data(&mut client);
    let file = file_uri(&path);
    client.open(&file, &text);
    let mut version = 1;
    let median = median_of("LilyPond check", || {
        version += 1;
        let digit = (version % 10).to_string();
        let words = format!("speed {digit}");
        client.change(&file, version, digit_at, &digit);
        let checked = |d: &[Diagnostic]| {
            d.iter()
                .any(|d| from_lilypond(d) && d.message.contains(&words))
        };
        client.wait_diagnostics(&file, WAIT, checked).1
    });
    assert_under("LilyPond check", median, Duration::from_millis(600));
}

#[test]
#[ignore = "a timing test with the real LilyPond"]
fn tidy_under_10ms() {
    let _serial = one_at_a_time();
    let mut client = real_client(false);
    let file = uri("/music/song.ly");
    client.open(&file, SONG);
    client.wait_diagnostics(&file, WAIT, |_| true);
    let options = json!({"tabSize": 2, "insertSpaces": true});
    let lsp = median_of("tidy, LSP formatting", || {
        let start = Instant::now();
        let reply = client.request_raw(
            "textDocument/formatting",
            json!({"textDocument": {"uri": file}, "options": options}),
        );
        let took = start.elapsed();
        assert!(reply.response_result.is_ok(), "{reply:?}");
        took
    });
    let cli = median_of("tidy, swara format", || {
        let start = Instant::now();
        let mut format = Command::new(env!("CARGO_BIN_EXE_swara"))
            .arg("format")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        format
            .stdin
            .take()
            .unwrap()
            .write_all(SONG.as_bytes())
            .unwrap();
        let output = format.wait_with_output().unwrap();
        let took = start.elapsed();
        assert!(output.status.success(), "{output:?}");
        took
    });
    assert_under("tidy, LSP formatting", lsp, Duration::from_millis(10));
    assert_under("tidy, swara format", cli, Duration::from_millis(10));
}

#[test]
#[ignore = "a timing test with the real LilyPond"]
fn completion_under_20ms() {
    let _serial = one_at_a_time();
    let mut client = real_client(false);
    wait_for_data(&mut client);
    let file = uri("/music/song.ly");
    // A backslash typed on a line of its own at the top of the melody, where a command can start.
    let text = SONG.replacen("mus = {\n", "mus = {\n  \\\n", 1);
    let line = text.lines().position(|l| l == "  \\").unwrap() as u32;
    client.open(&file, &text);
    client.wait_diagnostics(&file, WAIT, |_| true);
    let mut offered = Vec::new();
    let median = median_of("completion", || {
        let start = Instant::now();
        let reply = client.request_raw(
            "textDocument/completion",
            json!({"textDocument": {"uri": file}, "position": {"line": line, "character": 3}}),
        );
        let took = start.elapsed();
        offered = reply
            .response_result
            .ok()
            .and_then(|r| r.as_array().cloned())
            .unwrap_or_default();
        took
    });
    println!("completion: {} items offered", offered.len());
    assert!(
        offered.iter().any(|item| item["label"] == "\\relative"),
        "{} items",
        offered.len()
    );
    assert_under("completion", median, Duration::from_millis(20));
}

/// Saves `text` as `<name>.ly` again and again, timing each save to the build's report that the
/// PDF is in place beside it.
fn save_to_pdf(name: &str, text: &str) -> Duration {
    let dir = scratch(name);
    let path = dir.join(format!("{name}.ly"));
    fs::write(&path, text).unwrap();
    let pdf = dir.join(format!("{name}.pdf"));
    let mut client = real_client(false);
    let file = file_uri(&path);
    client.open(&file, text);
    median_of(&format!("PDF after save, {name}"), || {
        let start = Instant::now();
        client.save(&file);
        let finished = client.wait_notification(FINISHED, WAIT);
        let took = start.elapsed();
        assert_eq!(finished["success"], true, "{finished}");
        assert_eq!(finished["pdf"], pdf.to_str().unwrap(), "{finished}");
        took
    })
}

#[test]
#[ignore = "a timing test with the real LilyPond"]
fn pdf_after_save() {
    let _serial = one_at_a_time();
    let lesson_target = Duration::from_millis(300);
    let song_target = Duration::from_millis(1500);
    let lesson = save_to_pdf("lesson", &lesson());
    let song = save_to_pdf("song", SONG);
    // LilyPond's own engraving sets the floor here, so the targets are approximate, and only a
    // run over half as long again fails.
    assert!(
        lesson <= lesson_target.mul_f64(1.5) && song <= song_target.mul_f64(1.5),
        "lesson {:.0} ms against about {:.0} ms, song {:.0} ms against about {:.0} ms",
        ms(lesson),
        ms(lesson_target),
        ms(song),
        ms(song_target)
    );
}
