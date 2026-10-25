//! LilyPond's command data in the server: the notice when there is no LilyPond, the dump on a
//! helper thread, and the cache that saves the next start from dumping again. The fake `lilypond`
//! stands in for the real one.

// The fake is a shell script.
#![cfg(unix)]

mod client;

use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use client::{TestClient, uri};
use lsp_types::{Diagnostic, Position, Range};
use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(5);
const SETTLE: Duration = Duration::from_millis(500);
const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-lilypond.sh");
const MINI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/commanddata/mini.jsonl");
const NOT_FOUND: &str =
    "LilyPond not found: the LilyPond check, builds, preview and playback are off";

/// A fresh, empty folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp_commanddata").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn shown(client: &mut TestClient) -> Vec<String> {
    let shown = client.notifications("window/showMessage", SETTLE);
    shown.iter().map(|params| params["message"].as_str().unwrap().to_owned()).collect()
}

/// The server's summary of its command data, once it has loaded.
fn wait_for_data(client: &mut TestClient) -> Value {
    let deadline = Instant::now() + WAIT;
    loop {
        let reply = client.request_raw("swara/debugCommandData", json!({}));
        let summary = reply.response_result.expect("debugCommandData answers");
        if !summary.is_null() {
            return summary;
        }
        assert!(Instant::now() < deadline, "command data did not load within {WAIT:?}");
        sleep(Duration::from_millis(20));
    }
}

fn syntax_still_checked(client: &mut TestClient) {
    let file = uri("/music/lesson.ly");
    client.open(&file, "{ c d");
    let (diagnostics, _) = client.wait_diagnostics(&file, WAIT, |d| !d.is_empty());
    assert_eq!(diagnostics[0].message, "missing `}`");
}

#[test]
fn no_lilypond_degrades() {
    let options = json!({"autoCheck": true, "lilypond": "/nonexistent/bin/lilypond"});
    let mut client = TestClient::spawn(options);
    syntax_still_checked(&mut client);
    assert_eq!(shown(&mut client), [NOT_FOUND]);
}

#[test]
fn no_lilypond_on_path_degrades() {
    let empty = scratch("empty-path");
    let options = json!({"autoCheck": true, "lilypond": null});
    let mut client = TestClient::spawn_env(options, &[("PATH", empty.to_str().unwrap())]);
    syntax_still_checked(&mut client);
    assert_eq!(shown(&mut client), [NOT_FOUND]);
}

/// The fake answers `--version` with `version`, and copies `dump` wherever a dump asks.
fn start_fake(dir: &Path, version: &str, dump: Option<&str>) -> TestClient {
    start_fake_env(dir, version, dump, &[])
}

fn start_fake_env(
    dir: &Path,
    version: &str,
    dump: Option<&str>,
    extra: &[(&str, &str)],
) -> TestClient {
    let log = dir.join("fake.log");
    let cache = dir.join("cache");
    let mut env = vec![
        ("SWARA_FAKE_LOG", log.to_str().unwrap()),
        ("SWARA_FAKE_VERSION", version),
        ("SWARA_TEST_CACHE_DIR", cache.to_str().unwrap()),
    ];
    if let Some(dump) = dump {
        env.push(("SWARA_FAKE_DUMP", dump));
    }
    env.extend_from_slice(extra);
    let options = json!({"autoCheck": false, "lilypond": FAKE});
    TestClient::spawn_env(options, &env)
}

fn dumps(dir: &Path) -> usize {
    fs::read_to_string(dir.join("fake.log.dumps")).map_or(0, |log| log.lines().count())
}

fn cache_files(dir: &Path) -> usize {
    fs::read_dir(dir.join("cache").join("swara")).map_or(0, |entries| entries.count())
}

#[test]
fn command_data_loads_then_comes_from_cache() {
    let dir = scratch("cache");
    let mut client = start_fake(&dir, "2.26.0", Some(MINI));
    assert_eq!(wait_for_data(&mut client)["musicFunctions"], 8);
    assert!(shown(&mut client).is_empty());
    drop(client);
    assert_eq!((dumps(&dir), cache_files(&dir)), (1, 1));

    // The next start reads the cache, so the fake is never asked to dump.
    let mut client = start_fake(&dir, "2.26.0", None);
    assert_eq!(wait_for_data(&mut client)["musicFunctions"], 8);
    drop(client);
    assert_eq!((dumps(&dir), cache_files(&dir)), (1, 1));

    // Another version is another cache.
    let mut client = start_fake(&dir, "2.24.4", Some(MINI));
    wait_for_data(&mut client);
    drop(client);
    assert_eq!((dumps(&dir), cache_files(&dir)), (2, 2));
}

#[test]
fn failed_dump_is_logged_and_data_stays_absent() {
    let dir = scratch("failed");
    // Without a file to copy, the fake's dump writes nothing.
    let mut client = start_fake(&dir, "2.26.0", None);
    let deadline = Instant::now() + WAIT;
    let poll = Duration::from_millis(20);
    while client.notifications("window/logMessage", poll).is_empty() && Instant::now() < deadline {}
    let logged = client.notifications("window/logMessage", SETTLE);
    assert_eq!(logged.len(), 1, "{logged:?}");
    let message = logged[0]["message"].as_str().unwrap();
    assert!(message.starts_with("LilyPond's command data could not be read: "), "{message}");
    let reply = client.request_raw("swara/debugCommandData", json!({}));
    assert_eq!(reply.response_result.expect("debugCommandData answers"), Value::Null);
    assert_eq!((dumps(&dir), cache_files(&dir)), (1, 0));
    assert!(shown(&mut client).is_empty());
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists, and takes no pointers.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// The pids in a fake's log, the first word of each line.
fn pids(log: &Path) -> Vec<i32> {
    let text = fs::read_to_string(log).unwrap_or_default();
    text.lines().filter_map(|line| line.split_whitespace().next()?.parse().ok()).collect()
}

#[test]
fn shutdown_stops_a_running_dump() {
    let dir = scratch("shutdown");
    // The dump starts a grandchild, then sleeps far longer than the test runs.
    let client = start_fake_env(&dir, "2.26.0", None, &[("SWARA_FAKE_MODE", "sleep:30")]);
    let children = dir.join("fake.log.children");
    let deadline = Instant::now() + WAIT;
    while pids(&children).is_empty() {
        assert!(Instant::now() < deadline, "the dump did not start within {WAIT:?}");
        sleep(Duration::from_millis(20));
    }
    let mut started = pids(&dir.join("fake.log.dumps"));
    started.extend(pids(&children));
    assert_eq!(started.len(), 2, "{started:?}");
    drop(client);
    let deadline = Instant::now() + WAIT;
    while started.iter().any(|&pid| alive(pid)) {
        assert!(Instant::now() < deadline, "left running after shutdown: {started:?}");
        sleep(Duration::from_millis(20));
    }
}

fn has(diagnostics: &[Diagnostic], message: &str) -> bool {
    diagnostics.iter().any(|d| d.message == message)
}

#[test]
fn names_checked_once_data_loads() {
    let dir = scratch("names-load");
    let mut client = start_fake(&dir, "2.26.0", Some(MINI));
    // Opened while the dump runs, so the check waits for the data and then runs.
    let file = uri("/music/lesson.ly");
    client.open(&file, "{ \\melodyy }\n");
    client.wait_diagnostics(&file, WAIT, |d| has(d, "unknown command `\\melodyy`"));
}

#[test]
fn names_follow_an_open_include() {
    let dir = scratch("names-include");
    let mut client = start_fake(&dir, "2.26.0", Some(MINI));
    wait_for_data(&mut client);
    let defs = uri("/music/defs.ily");
    let main = uri("/music/main.ly");
    client.open(&defs, "% nothing yet\n");
    client.open(&main, "\\include \"defs.ily\"\n{ \\tune }\n");
    client.wait_diagnostics(&main, WAIT, |d| has(d, "unknown command `\\tune`"));
    let end = Position::new(1, 0);
    client.change(&defs, 2, Range::new(end, end), "tune = { c }\n");
    client.wait_diagnostics(&main, WAIT, <[Diagnostic]>::is_empty);
}
