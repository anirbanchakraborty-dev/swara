//! A stand-in `lilypond`, `tests/fixtures/fake-lilypond.sh`, and the checks the tests make on the
//! processes it starts.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::json;

use super::TestClient;

pub const FAKE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/fake-lilypond.sh"
);

/// The fake's record of its runs.
pub struct Fake {
    pub log: PathBuf,
}

impl Fake {
    /// One line per run: its pid, then its arguments.
    pub fn runs(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    /// Every process the fake started: each run's own pid and each grandchild's.
    pub fn pids(&self) -> Vec<i32> {
        let mut children = self.log.clone().into_os_string();
        children.push(".children");
        let children = fs::read_to_string(children).unwrap_or_default();
        let runs = self.runs();
        let own = runs
            .iter()
            .map(|run| run.split(' ').next().unwrap().to_owned());
        own.chain(children.lines().map(String::from))
            .map(|pid| pid.parse().unwrap())
            .collect()
    }
}

/// A server whose `lilypond` is the fake, set to `mode`, with its log in `dir`.
pub fn start(dir: &Path, mode: &str, auto_check: bool, env: &[(&str, &str)]) -> (TestClient, Fake) {
    let log = dir.join("fake.log");
    let options = json!({"autoCheck": auto_check, "lilypond": FAKE, "includePaths": []});
    let mut env = env.to_vec();
    env.push(("SWARA_FAKE_LOG", log.to_str().unwrap()));
    env.push(("SWARA_FAKE_MODE", mode));
    (TestClient::spawn_env(options, &env), Fake { log })
}

/// Polls `done` every 5 ms until it holds or `timeout` passes.
pub fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        sleep(Duration::from_millis(5));
    }
}

/// The pid of a process that has ended.
pub fn dead_pid() -> u32 {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

pub fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 delivers nothing; it only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Waits for the fake to have started `count` processes, then for all of them to be gone.
pub fn assert_all_gone(fake: &Fake, count: usize) {
    let started = wait_until(Duration::from_secs(5), || fake.pids().len() >= count);
    assert!(started, "pids: {:?}", fake.pids());
    let pids = fake.pids();
    let gone = wait_until(Duration::from_secs(3), || {
        pids.iter().all(|&pid| !alive(pid))
    });
    let left: Vec<_> = pids.iter().filter(|&&pid| alive(pid)).collect();
    assert!(gone, "still running: {left:?}");
}
