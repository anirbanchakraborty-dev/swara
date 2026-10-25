//! Starting and stopping LilyPond runs. A helper thread per run feeds its standard input and reads
//! its standard error, then hands the text to the server's loop, which never blocks on a child.
//! On Unix every run leads its own process group, so stopping it also stops whatever it started.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use lsp_types::Uri;

/// How long typing must pause before the check starts.
pub const PAUSE: Duration = Duration::from_millis(330);

/// How long a check may run before it is stopped, as when Scheme typed into a lesson never ends.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a build or a preview may run. A long score takes a while to engrave, so this only
/// catches a run that will never finish.
const BUILD_TIMEOUT: Duration = Duration::from_secs(300);

/// The settings chosen in `docs/spikes/lilypond-check.md`: every score is interpreted with its
/// engravers taken out, so nothing is laid out and nothing is written.
const CHECK_SETTINGS: &str = include_str!("../../assets/lilypond/check-settings.ly");

/// Numbers runs and their output folders, so each is told apart from every other.
static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunKind {
    Check,
    // The save build and the preview construct these, in a later task.
    #[allow(dead_code)]
    Build,
    #[allow(dead_code)]
    Preview,
}

/// What to run, and for which version of which file.
pub struct RunSpec {
    pub kind: RunKind,
    pub uri: Uri,
    pub version: i32,
    /// The text for standard input, when the file argument is `-`.
    pub stdin: Option<String>,
    /// The program, then its arguments.
    pub args: Vec<OsString>,
    /// The folder the run writes into, which the caller removes once the run is over.
    pub out_dir: PathBuf,
}

/// A run in progress. Whoever holds it must `stop` it, which also reaps it.
pub struct Running {
    pub id: u64,
    pub child: Child,
    pub deadline: Instant,
}

/// What a run printed, sent once its standard error closes.
pub struct RunResult {
    pub id: u64,
    pub stderr: String,
    /// Filled in by the receiver once it has reaped the run.
    // The save build reports it, in a later task.
    #[allow(dead_code)]
    pub status: Option<ExitStatus>,
}

impl Running {
    /// Stops the run and everything it started, then reaps it. Safe to call on a run that has
    /// already exited.
    pub fn stop(mut self) -> Option<ExitStatus> {
        #[cfg(unix)]
        {
            // The child leads its own group, so the group has the child's id. Until `wait` reaps
            // the child, that id cannot belong to anything else.
            let group = self.child.id() as libc::pid_t;
            // SAFETY: killpg only sends a signal, and takes no pointers.
            unsafe { libc::killpg(group, libc::SIGKILL) };
        }
        #[cfg(not(unix))]
        let _ = self.child.kill();
        self.child.wait().ok()
    }
}

/// The check of one version of a file, with its text on standard input. The arguments are the
/// ones `docs/spikes/lilypond-check.md` records, with the settings file and every include folder
/// given as absolute paths.
pub fn check_spec(
    uri: &Uri,
    lilypond: &Path,
    file: &Path,
    text: &str,
    version: i32,
    include_paths: &[PathBuf],
) -> RunSpec {
    let out_dir = work_dir().join(format!("run-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let mut settings = OsString::from("-dinclude-settings=");
    settings.push(settings_path());
    let mut args: Vec<OsString> =
        vec![lilypond.into(), "-l".into(), "WARNING".into(), settings, "-I".into()];
    args.push(file.parent().unwrap_or(Path::new("/")).into());
    for folder in include_paths {
        args.push("-I".into());
        args.push(folder.into());
    }
    args.push("-o".into());
    args.push(out_dir.join(file.file_stem().unwrap_or("swara".as_ref())).into());
    args.push("-".into());
    RunSpec {
        kind: RunKind::Check,
        uri: uri.clone(),
        version,
        stdin: Some(text.to_owned()),
        args,
        out_dir,
    }
}

/// Starts a run. Its result arrives on `results` once its standard error closes, which is when it
/// exits or is stopped.
pub fn spawn(spec: RunSpec, results: Sender<RunResult>) -> io::Result<Running> {
    fs::create_dir_all(&spec.out_dir)?;
    if spec.kind == RunKind::Check {
        write_settings()?;
    }
    let (program, args) = spec.args.split_first().expect("a run names its program");
    let mut command = Command::new(program);
    command
        .args(args)
        .env("LC_ALL", "C")
        .stdin(if spec.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn()?;
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let stdin = child.stdin.take();
    let stderr = child.stderr.take();
    let text = spec.stdin;
    thread::spawn(move || {
        let mut printed = Vec::new();
        // Writing and reading at once, since a run that fills its standard error before it has
        // read all of its input would otherwise wait on this thread forever.
        thread::scope(|scope| {
            if let (Some(mut pipe), Some(text)) = (stdin, text) {
                // A run that stops reading early, on a fatal error, makes the write fail, which
                // says nothing its standard error will not.
                scope.spawn(move || {
                    let _ = pipe.write_all(text.as_bytes());
                });
            }
            if let Some(mut pipe) = stderr {
                let _ = pipe.read_to_end(&mut printed);
            }
        });
        let stderr = String::from_utf8_lossy(&printed).into_owned();
        // The server may have shut down, and with it the receiver.
        let _ = results.send(RunResult { id, stderr, status: None });
    });
    let timeout = match spec.kind {
        RunKind::Check => check_timeout(),
        RunKind::Build | RunKind::Preview => BUILD_TIMEOUT,
    };
    Ok(Running { id, child, deadline: Instant::now() + timeout })
}

/// `CHECK_TIMEOUT`, or under `SWARA_TEST=1` the milliseconds in `SWARA_TEST_CHECK_TIMEOUT_MS`.
pub fn check_timeout() -> Duration {
    let testing = std::env::var_os("SWARA_TEST").is_some_and(|v| v == "1");
    let millis = std::env::var("SWARA_TEST_CHECK_TIMEOUT_MS").ok().filter(|_| testing);
    millis.and_then(|m| m.parse().ok()).map_or(CHECK_TIMEOUT, Duration::from_millis)
}

/// Removes this server's folder in the system's temporary folder, with everything in it.
pub fn remove_work_dir() {
    let _ = fs::remove_dir_all(work_dir());
}

/// The folder for this server's settings file and run output, one per process.
fn work_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| std::env::temp_dir().join(format!("swara-{}", std::process::id())))
}

fn settings_path() -> PathBuf {
    work_dir().join("check-settings.ly")
}

/// Writes the settings file unless it is there. A cleaner can empty the temporary folder under a
/// server that runs for days, so this looks each time. The file appears whole, through a rename,
/// so a check running at the same moment never reads half of it.
fn write_settings() -> io::Result<()> {
    let path = settings_path();
    if path.is_file() {
        return Ok(());
    }
    let partial = work_dir().join("check-settings.ly.partial");
    fs::write(&partial, CHECK_SETTINGS)?;
    fs::rename(partial, path)
}
