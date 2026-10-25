//! Starting and stopping LilyPond runs. A helper thread per run feeds its standard input and reads
//! its standard error, then hands the text to the server's loop, which never blocks on a child.
//! On Unix every run leads its own process group, so stopping it also stops whatever it started.

use std::collections::hash_map::RandomState;
use std::ffi::OsString;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
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

/// The settings file's name in the work folder.
const SETTINGS_NAME: &str = "check-settings.ly";

/// Numbers runs and their output folders, so each is told apart from every other.
static NEXT: AtomicU64 = AtomicU64::new(1);

/// This server's work folder, once it has made one.
static WORK_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RunKind {
    Check,
    Build,
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
}

impl Running {
    /// Stops the run and everything it started, then reaps it. Safe to call on a run that has
    /// already exited.
    pub fn stop(mut self) -> Option<ExitStatus> {
        kill_group(&mut self.child)
    }
}

/// Stops a child that leads its own group, and everything it started, then reaps it. Safe to call
/// on a child that has already exited.
pub fn kill_group(child: &mut Child) -> Option<ExitStatus> {
    #[cfg(unix)]
    {
        // The child leads its own group, so the group has the child's id. Until `wait` reaps the
        // child, that id cannot belong to anything else.
        let group = child.id() as libc::pid_t;
        // SAFETY: killpg only sends a signal, and takes no pointers.
        unsafe { libc::killpg(group, libc::SIGKILL) };
    }
    #[cfg(not(unix))]
    let _ = child.kill();
    child.wait().ok()
}

/// The check of one version of a file, with its text on standard input. The arguments are the
/// ones `docs/spikes/lilypond-check.md` records, with the settings file and every include folder
/// given as absolute paths. Fails when the work folder cannot be made.
pub fn check_spec(
    uri: &Uri,
    lilypond: &Path,
    file: &Path,
    text: &str,
    version: i32,
    include_paths: &[PathBuf],
) -> io::Result<RunSpec> {
    let dir = work_dir()?;
    let out_dir = dir.join(format!("run-{}", next_number()));
    let mut settings = OsString::from("-dinclude-settings=");
    settings.push(dir.join(SETTINGS_NAME));
    let mut args: Vec<OsString> = vec![
        lilypond.into(),
        "-l".into(),
        "WARNING".into(),
        settings,
        "-I".into(),
    ];
    args.push(file.parent().unwrap_or(Path::new("/")).into());
    for folder in include_paths {
        args.push("-I".into());
        args.push(folder.into());
    }
    args.push("-o".into());
    args.push(
        out_dir
            .join(file.file_stem().unwrap_or("swara".as_ref()))
            .into(),
    );
    args.push("-".into());
    Ok(RunSpec {
        kind: RunKind::Check,
        uri: uri.clone(),
        version,
        stdin: Some(text.to_owned()),
        args,
        out_dir,
    })
}

/// Starts a run. Its result arrives on `results` once its standard error closes, which is when it
/// exits or is stopped.
pub fn spawn(spec: RunSpec, results: Sender<RunResult>) -> io::Result<Running> {
    fs::create_dir_all(&spec.out_dir)?;
    let (program, args) = spec.args.split_first().expect("a run names its program");
    let mut command = Command::new(program);
    english(command.args(args))
        .stdin(if spec.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn()?;
    let id = next_number();
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
        let _ = results.send(RunResult { id, stderr });
    });
    let timeout = match spec.kind {
        RunKind::Check => check_timeout(),
        RunKind::Build | RunKind::Preview => build_timeout(),
    };
    Ok(Running {
        id,
        child,
        deadline: Instant::now() + timeout,
    })
}

/// The next number from the one sequence that tells runs and their folders apart.
pub(crate) fn next_number() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Makes a LilyPond run report in English, which the message parser keys on. `C.UTF-8` rather
/// than `C`, because under `C` LilyPond cannot open a file whose name is not ASCII. `LANGUAGE`
/// is removed because GNU gettext reads it before `LC_ALL` to pick the language of messages.
pub(crate) fn english(command: &mut Command) -> &mut Command {
    command.env("LC_ALL", "C.UTF-8").env_remove("LANGUAGE")
}

/// `CHECK_TIMEOUT`, or under `SWARA_TEST=1` the milliseconds in `SWARA_TEST_CHECK_TIMEOUT_MS`.
pub fn check_timeout() -> Duration {
    test_millis("SWARA_TEST_CHECK_TIMEOUT_MS").map_or(CHECK_TIMEOUT, Duration::from_millis)
}

/// `BUILD_TIMEOUT`, or under `SWARA_TEST=1` the milliseconds in `SWARA_TEST_BUILD_TIMEOUT_MS`.
pub fn build_timeout() -> Duration {
    test_millis("SWARA_TEST_BUILD_TIMEOUT_MS").map_or(BUILD_TIMEOUT, Duration::from_millis)
}

/// The milliseconds in the variable `name`, read only under `SWARA_TEST=1`.
fn test_millis(name: &str) -> Option<u64> {
    let testing = std::env::var_os("SWARA_TEST").is_some_and(|v| v == "1");
    std::env::var(name)
        .ok()
        .filter(|_| testing)
        .and_then(|m| m.parse().ok())
}

/// Removes this server's folder in the system's temporary folder, with everything in it.
pub fn remove_work_dir() {
    let dir = WORK_DIR
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    if let Some(dir) = dir {
        let _ = fs::remove_dir_all(dir);
    }
}

/// The folder for this server's settings file and run output, one per process. A cleaner can
/// empty the temporary folder under a server that runs for days, so this looks each time, and
/// makes a new folder when the old one is gone or no longer private.
pub(crate) fn work_dir() -> io::Result<PathBuf> {
    let mut current = WORK_DIR.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(dir) = current.as_ref().filter(|dir| is_private(dir)) {
        return Ok(dir.clone());
    }
    let dir = make_work_dir(&std::env::temp_dir())?;
    *current = Some(dir.clone());
    Ok(dir)
}

/// Makes a folder in `base` that only this user can open, with the settings file in it. LilyPond
/// runs that file as Scheme, and on Linux the temporary folder is shared by every user, so the
/// name cannot be guessed, and a name already taken is passed over rather than used.
fn make_work_dir(base: &Path) -> io::Result<PathBuf> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    for attempt in 0..16 {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(attempt);
        let dir = base.join(format!(
            "swara-{}-{:016x}",
            std::process::id(),
            hasher.finish()
        ));
        match builder.create(&dir) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let written = options.open(dir.join(SETTINGS_NAME));
        if let Err(error) = written.and_then(|mut file| file.write_all(CHECK_SETTINGS.as_bytes())) {
            let _ = fs::remove_dir_all(&dir);
            return Err(error);
        }
        return Ok(dir);
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "every name tried for the work folder was taken",
    ))
}

/// Whether `dir` is still a work folder this server can trust: a folder and not a link to one,
/// owned by this user and closed to everyone else, holding a settings file that is the same.
#[cfg(unix)]
fn is_private(dir: &Path) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    // SAFETY: geteuid takes nothing and cannot fail.
    let me = unsafe { libc::geteuid() };
    let closed = |path: &Path, kind: fn(&fs::FileType) -> bool| {
        fs::symlink_metadata(path).is_ok_and(|meta| {
            kind(&meta.file_type()) && meta.uid() == me && meta.permissions().mode() & 0o077 == 0
        })
    };
    closed(dir, fs::FileType::is_dir) && closed(&dir.join(SETTINGS_NAME), fs::FileType::is_file)
}

#[cfg(not(unix))]
fn is_private(dir: &Path) -> bool {
    dir.join(SETTINGS_NAME).is_file()
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::*;

    /// A fresh folder for one test, standing in for the shared temporary folder.
    fn base(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swara-unit-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn mode(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn runs_read_utf8_paths_and_report_in_english() {
        let mut command = Command::new("lilypond");
        english(&mut command);
        let envs: Vec<_> = command.get_envs().collect();
        assert!(
            envs.contains(&("LC_ALL".as_ref(), Some("C.UTF-8".as_ref()))),
            "{envs:?}"
        );
        assert!(envs.contains(&("LANGUAGE".as_ref(), None)), "{envs:?}");
    }

    #[test]
    fn work_dir_only_this_user_can_open() {
        let base = base("private");
        let dir = make_work_dir(&base).unwrap();
        assert_eq!(mode(&dir), 0o700);
        let settings = dir.join(SETTINGS_NAME);
        assert_eq!(mode(&settings), 0o600);
        assert_eq!(fs::read_to_string(settings).unwrap(), CHECK_SETTINGS);
        assert!(is_private(&dir));
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn folder_someone_else_could_write_is_not_trusted() {
        let base = base("planted");
        let planted = base.join("planted");
        fs::create_dir(&planted).unwrap();
        fs::write(planted.join(SETTINGS_NAME), "#(system \"true\")").unwrap();
        fs::set_permissions(&planted, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(!is_private(&planted));
        // A link to a private folder is not that folder.
        let private = make_work_dir(&base).unwrap();
        let link = base.join("link");
        symlink(&private, &link).unwrap();
        assert!(!is_private(&link));
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn folder_without_its_settings_is_replaced() {
        let base = base("emptied");
        let dir = make_work_dir(&base).unwrap();
        fs::remove_file(dir.join(SETTINGS_NAME)).unwrap();
        assert!(!is_private(&dir));
        let again = make_work_dir(&base).unwrap();
        assert_ne!(again, dir);
        assert!(is_private(&again));
        fs::remove_dir_all(base).unwrap();
    }
}
