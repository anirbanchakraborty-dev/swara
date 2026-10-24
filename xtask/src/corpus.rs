//! The comparison corpus. `fetch` gathers LilyPond's own example files, its init files, a pinned set of
//! Mutopia pieces and the owner's lessons under `bench/corpus/raw/<source>/`. `filter` sorts every `.ly`
//! file there by whether LilyPond 2.26 accepts it, converting old syntax with `convert-ly` where that
//! helps, and writes a manifest that names each file without holding any of its text.

use crate::sha256;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// LilyPond's sources, which supply `input/regression` and `Documentation/snippets`.
const TARBALL_URL: &str = "https://lilypond.org/download/sources/v2.26/lilypond-2.26.0.tar.gz";
const TARBALL: &str = "lilypond-2.26.0.tar.gz";
const TOP: &str = "lilypond-2.26.0";
const MUTOPIA_RAW: &str = "https://raw.githubusercontent.com/MutopiaProject/MutopiaProject/";
/// The name prefix of the copy a conversion runs on. It sits beside the original so that its includes
/// resolve the same way, and the leading dot keeps it out of the walk.
const WORKING: &str = ".swara-convert-";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Accepted,
    Converted,
    Rejected,
    Timeout,
}

impl Class {
    const ALL: [Class; 4] = [
        Class::Accepted,
        Class::Converted,
        Class::Rejected,
        Class::Timeout,
    ];

    fn name(self) -> &'static str {
        match self {
            Class::Accepted => "accepted",
            Class::Converted => "converted",
            Class::Rejected => "rejected",
            Class::Timeout => "timeout",
        }
    }

    /// The folder the file is copied to. A converted file is accepted, in its converted form.
    fn folder(self) -> &'static str {
        match self {
            Class::Accepted | Class::Converted => "accepted",
            Class::Rejected => "rejected",
            Class::Timeout => "timeout",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed,
    TimedOut,
}

/// Runs LilyPond's check, or `convert-ly`, on one file. It is a trait so that the sorting is tested
/// without LilyPond.
pub trait Runner {
    /// Checks `file` from the folder `cwd`.
    fn run(&self, file: &Path, cwd: &Path, timeout: Duration) -> Outcome;
    /// Rewrites `file` in place into current syntax, from the folder `cwd`.
    fn convert(&self, file: &Path, cwd: &Path, timeout: Duration) -> Outcome;
}

#[derive(Debug)]
pub struct Entry {
    /// The path under `raw/`, starting with the source, with `/` between the parts.
    pub path: String,
    pub source: String,
    pub bytes: u64,
    /// The SHA-256 of the original bytes.
    pub sha256: String,
    pub class: Class,
    /// For a converted file, the SHA-256 of the converted text kept in `accepted/`.
    pub converted_sha256: Option<String>,
}

/// Sorts every `.ly` file under `raw/<source>/` into `out/{accepted,rejected,timeout}/<source>/`, with
/// `jobs` files in flight at once, and returns one entry per file in path order. Files are read and
/// copied as bytes, so text that is not UTF-8 is sorted like any other. An `.ily` file is not sorted,
/// because it cannot be compiled alone; it stays in `raw/` for the files that include it.
pub fn filter<R: Runner + Sync>(
    runner: &R,
    raw: &Path,
    out: &Path,
    jobs: usize,
    timeout: Duration,
) -> io::Result<Vec<Entry>> {
    let raw = raw.canonicalize()?;
    for class in Class::ALL {
        match fs::remove_dir_all(out.join(class.folder())) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    let mut files = Vec::new();
    for source in fs::read_dir(&raw)? {
        let source = source?;
        if source.file_type()?.is_dir() && !hidden(&source.file_name()) {
            collect(&source.path(), &mut files)?;
        }
    }
    files.sort();

    let next = AtomicUsize::new(0);
    let done = Mutex::new(Vec::with_capacity(files.len()));
    thread::scope(|s| {
        for _ in 0..jobs.max(1) {
            s.spawn(|| {
                while let Some(file) = files.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let result = sort_one(runner, &raw, file, out, timeout);
                    let done = &mut done.lock().unwrap();
                    done.push(result);
                    if done.len() % 100 == 0 || done.len() == files.len() {
                        eprintln!("corpus filter: {}", progress(done, files.len()));
                    }
                }
            });
        }
    });
    let mut entries = done
        .into_inner()
        .unwrap()
        .into_iter()
        .collect::<io::Result<Vec<_>>>()?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn hidden(name: &std::ffi::OsStr) -> bool {
    name.as_encoded_bytes().starts_with(b".")
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for item in fs::read_dir(dir)? {
        let item = item?;
        if hidden(&item.file_name()) {
            continue;
        }
        let path = item.path();
        if item.file_type()?.is_dir() {
            collect(&path, files)?;
        } else if path.extension().is_some_and(|x| x == "ly") && fs::metadata(&path)?.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn progress(done: &[io::Result<Entry>], total: usize) -> String {
    let mut counts = [0usize; 4];
    for entry in done.iter().flatten() {
        counts[entry.class as usize] += 1;
    }
    let per_class: Vec<String> = Class::ALL
        .iter()
        .map(|c| format!("{} {}", c.name(), counts[*c as usize]))
        .collect();
    format!("{} of {total} done ({})", done.len(), per_class.join(", "))
}

fn sort_one<R: Runner>(
    runner: &R,
    raw: &Path,
    file: &Path,
    out: &Path,
    timeout: Duration,
) -> io::Result<Entry> {
    let rel = file.strip_prefix(raw).expect("the walk starts at raw");
    let parts: Vec<_> = rel.iter().map(|p| p.to_string_lossy()).collect();
    let original = fs::read(file)?;
    let dir = file.parent().expect("a file under raw has a folder");
    let mut converted = None;
    let class = match runner.run(file, dir, timeout) {
        Outcome::Passed => Class::Accepted,
        Outcome::TimedOut => Class::Timeout,
        Outcome::Failed => match convert_and_run(runner, file, dir, timeout)? {
            (Outcome::Passed, text) => {
                converted = Some(text);
                Class::Converted
            }
            (Outcome::TimedOut, _) => Class::Timeout,
            (Outcome::Failed, _) => Class::Rejected,
        },
    };
    let dest = out.join(class.folder()).join(rel);
    fs::create_dir_all(dest.parent().expect("dest has a folder"))?;
    fs::write(&dest, converted.as_deref().unwrap_or(&original))?;
    Ok(Entry {
        path: parts.join("/"),
        source: parts[0].to_string(),
        bytes: original.len() as u64,
        sha256: sha256::hex(&original),
        class,
        converted_sha256: converted.as_deref().map(sha256::hex),
    })
}

/// Converts a copy of `file` beside it and checks the copy. Returns the check's outcome, or the
/// conversion's when that did not pass, with the converted text.
fn convert_and_run<R: Runner>(
    runner: &R,
    file: &Path,
    dir: &Path,
    timeout: Duration,
) -> io::Result<(Outcome, Vec<u8>)> {
    let name = file
        .file_name()
        .expect("a file has a name")
        .to_string_lossy();
    let copy = dir.join(format!("{WORKING}{name}"));
    fs::copy(file, &copy)?;
    let outcome = match runner.convert(&copy, dir, timeout) {
        Outcome::Passed => runner.run(&copy, dir, timeout),
        other => other,
    };
    let text = fs::read(&copy)?;
    fs::remove_file(&copy)?;
    // convert-ly -e keeps the text it replaced under the same name with a trailing tilde.
    let _ = fs::remove_file(dir.join(format!("{WORKING}{name}~")));
    Ok((outcome, text))
}

/// The real runner: LilyPond with Swara's check settings, from the LilyPond check spike, and
/// `convert-ly`. Every run sets `LC_ALL=C` and passes absolute paths, options first.
pub struct LilyPond {
    pub lilypond: PathBuf,
    pub convert_ly: PathBuf,
    /// `assets/lilypond/check-settings.ly`, as an absolute path.
    pub settings: PathBuf,
    /// Where `-o` points. The check settings never write output, so it stays empty.
    pub tmp: PathBuf,
    pub serial: AtomicUsize,
}

impl Runner for LilyPond {
    fn run(&self, file: &Path, cwd: &Path, timeout: Duration) -> Outcome {
        let mut settings = OsString::from("-dinclude-settings=");
        settings.push(&self.settings);
        let n = self.serial.fetch_add(1, Ordering::Relaxed);
        let mut cmd = Command::new(&self.lilypond);
        cmd.args(["-l", "WARNING"])
            .arg(settings)
            .arg("-I")
            .arg(cwd)
            .arg("-o")
            .arg(self.tmp.join(n.to_string()))
            .arg(file);
        wait(cmd, cwd, timeout)
    }

    fn convert(&self, file: &Path, cwd: &Path, timeout: Duration) -> Outcome {
        let mut cmd = Command::new(&self.convert_ly);
        cmd.arg("-e").arg(file);
        wait(cmd, cwd, timeout)
    }
}

/// Runs `cmd` in `cwd` and waits for it, killing it at the limit. Its output is discarded rather than
/// piped, so a run that prints a great deal cannot stall on a full pipe and pass for a timeout.
fn wait(mut cmd: Command, cwd: &Path, timeout: Duration) -> Outcome {
    let child = cmd
        .current_dir(cwd)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return Outcome::Failed;
    };
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Outcome::Passed,
            Ok(Some(_)) | Err(_) => return Outcome::Failed,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Outcome::TimedOut;
            }
        }
    }
}

/// The committed manifest: each file's path, source, size, hash and class, and the counts per source
/// and class. It holds no text from any file.
pub fn manifest_json(entries: &[Entry], mutopia_commit: &str, timeout_secs: u64) -> String {
    let mut counts: BTreeMap<&str, [usize; 4]> = BTreeMap::new();
    for e in entries {
        counts.entry(&e.source).or_default()[e.class as usize] += 1;
    }
    let summary: Vec<String> = counts
        .iter()
        .map(|(source, n)| {
            let per_class: Vec<String> = Class::ALL
                .iter()
                .map(|c| format!("\"{}\": {}", c.name(), n[*c as usize]))
                .collect();
            format!("    {}: {{{}}}", json_str(source), per_class.join(", "))
        })
        .collect();
    let files: Vec<String> = entries
        .iter()
        .map(|e| {
            let converted = match &e.converted_sha256 {
                Some(h) => format!(", \"converted_sha256\": {}", json_str(h)),
                None => String::new(),
            };
            format!(
                "    {{\"path\": {}, \"source\": {}, \"bytes\": {}, \"sha256\": {}, \"class\": {}{converted}}}",
                json_str(&e.path),
                json_str(&e.source),
                e.bytes,
                json_str(&e.sha256),
                json_str(e.class.name()),
            )
        })
        .collect();
    format!(
        "{{\n  \"lilypond\": \"2.26.0\",\n  \"mutopia_commit\": {},\n  \"timeout_secs\": {timeout_secs},\n  \"summary\": {{\n{}\n  }},\n  \"files\": [\n{}\n  ]\n}}\n",
        json_str(mutopia_commit),
        summary.join(",\n"),
        files.join(",\n"),
    )
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The commit and the path under `ftp/` that a Mutopia URL names. `None` for a URL outside the
/// project's raw tree, at anything but a full commit hash, or with a path that climbs out with `..`.
pub fn mutopia_dest(url: &str) -> Option<(String, PathBuf)> {
    let (commit, path) = url.strip_prefix(MUTOPIA_RAW)?.split_once("/ftp/")?;
    let full_hash = commit.len() == 40
        && commit
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    let inside = path
        .split('/')
        .all(|part| !part.is_empty() && part != "." && part != "..");
    (full_hash && inside).then(|| (commit.to_string(), PathBuf::from(path)))
}

/// The Mutopia URLs in `bench/mutopia.txt`, with the one commit they all pin.
fn mutopia_list(root: &Path) -> Result<(String, Vec<(String, PathBuf)>), String> {
    let list = root.join("bench/mutopia.txt");
    let text = fs::read_to_string(&list).map_err(|e| format!("{}: {e}", list.display()))?;
    let mut commit = None;
    let mut pieces = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (sha, path) = mutopia_dest(line)
            .ok_or_else(|| format!("bench/mutopia.txt: not a pinned Mutopia URL: {line}"))?;
        if commit.get_or_insert_with(|| sha.clone()) != &sha {
            return Err(format!("bench/mutopia.txt: more than one commit: {line}"));
        }
        pieces.push((line.to_string(), path));
    }
    Ok((commit.ok_or("bench/mutopia.txt lists no pieces")?, pieces))
}

fn run_ok(cmd: &mut Command) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("{cmd:?}: {e}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("{cmd:?}: {status}"))
}

fn io_err(path: &Path) -> impl Fn(io::Error) -> String + '_ {
    move |e| format!("{}: {e}", path.display())
}

/// LilyPond's data folder, `<prefix>/share/lilypond/2.26.0`, found beside the `lilypond` on `PATH`.
fn lilypond_datadir() -> Result<PathBuf, String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join("lilypond"))
        .find(|p| p.is_file())
        .and_then(|p| Some(p.parent()?.parent()?.join("share/lilypond/2.26.0")))
        .filter(|d| d.is_dir())
        .ok_or_else(|| "no share/lilypond/2.26.0 beside the lilypond on PATH".to_string())
}

/// Copies every file under `from` whose extension is in `exts` to the same place under `to`.
fn copy_tree(from: &Path, to: &Path, exts: &[&str], deep: bool) -> Result<usize, String> {
    let mut copied = 0;
    for item in fs::read_dir(from).map_err(io_err(from))? {
        let path = item.map_err(io_err(from))?.path();
        let dest = to.join(path.file_name().expect("a listed entry has a name"));
        if path.is_dir() && deep {
            copied += copy_tree(&path, &dest, exts, deep)?;
        } else if path.is_file()
            && path
                .extension()
                .is_some_and(|x| exts.iter().any(|e| x == *e))
        {
            fs::create_dir_all(to).map_err(io_err(to))?;
            fs::copy(&path, &dest).map_err(io_err(&dest))?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// `cargo xtask corpus fetch`: gathers the five sources under `bench/corpus/raw/`. Parts already there
/// are kept, except the lessons, which are copied afresh from `$SWARA_LESSONS_DIR` when it is set.
pub fn fetch(root: &Path) -> Result<(), String> {
    let corpus = root.join("bench/corpus");
    let raw = corpus.join("raw");
    let download = corpus.join("download");
    fs::create_dir_all(&raw).map_err(io_err(&raw))?;
    fs::create_dir_all(&download).map_err(io_err(&download))?;

    let tarball = download.join(TARBALL);
    if !tarball.is_file() {
        let part = download.join(format!("{TARBALL}.part"));
        run_ok(
            Command::new("curl")
                .args(["-fsSL", "-o"])
                .arg(&part)
                .arg(TARBALL_URL),
        )?;
        fs::rename(&part, &tarball).map_err(io_err(&tarball))?;
    }
    for (source, inner) in [
        ("regression", "input/regression"),
        ("snippets", "Documentation/snippets"),
    ] {
        let dest = raw.join(source);
        if !dest.is_dir() {
            run_ok(
                Command::new("tar")
                    .arg("-xzf")
                    .arg(&tarball)
                    .arg("-C")
                    .arg(&download)
                    .arg(format!("{TOP}/{inner}")),
            )?;
            fs::rename(download.join(TOP).join(inner), &dest).map_err(io_err(&dest))?;
        }
    }

    let init = raw.join("init");
    if !init.is_dir() {
        copy_tree(&lilypond_datadir()?.join("ly"), &init, &["ly"], false)?;
    }

    let (_, pieces) = mutopia_list(root)?;
    let mut config = String::new();
    for (url, path) in &pieces {
        let dest = raw.join("mutopia").join(path);
        if !dest.is_file() {
            config += &format!(
                "url = {}\noutput = {}\n",
                json_str(url),
                json_str(&dest.to_string_lossy())
            );
        }
    }
    if !config.is_empty() {
        let file = download.join("mutopia.curl");
        fs::write(&file, config).map_err(io_err(&file))?;
        run_ok(
            Command::new("curl")
                .args(["-fsSL", "--create-dirs", "--remove-on-error"])
                .args(["--parallel", "--parallel-max", "8", "-K"])
                .arg(&file),
        )?;
    }

    let lessons = raw.join("lessons");
    match fs::remove_dir_all(&lessons) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(io_err(&lessons)(e)),
        _ => {}
    }
    if let Some(dir) = std::env::var_os("SWARA_LESSONS_DIR") {
        copy_tree(Path::new(&dir), &lessons, &["ly", "ily"], true)?;
    }

    for source in ["regression", "snippets", "init", "mutopia", "lessons"] {
        let mut files = Vec::new();
        if raw.join(source).is_dir() {
            collect(&raw.join(source), &mut files).map_err(io_err(&raw))?;
        }
        println!("{source}: {} .ly files", files.len());
    }
    Ok(())
}

/// `cargo xtask corpus filter`: sorts `bench/corpus/raw/` into `bench/corpus/{accepted,rejected,timeout}/`
/// and writes `bench/results/corpus-manifest.json`.
pub fn run_filter(root: &Path, jobs: usize, timeout: Duration) -> Result<(), String> {
    let corpus = root.join("bench/corpus");
    let raw = corpus.join("raw");
    if !raw.is_dir() {
        return Err("bench/corpus/raw is missing: run `cargo xtask corpus fetch` first".into());
    }
    // Under any other version every file would be sorted for the wrong reason.
    let version = Command::new("lilypond")
        .arg("--version")
        .output()
        .map_err(|e| format!("lilypond: {e}"))?;
    if !String::from_utf8_lossy(&version.stdout).contains("GNU LilyPond 2.26.0") {
        return Err("the lilypond on PATH is not 2.26.0".into());
    }
    let (commit, _) = mutopia_list(root)?;
    let tmp = std::env::temp_dir().join(format!("swara-corpus-{}", std::process::id()));
    fs::create_dir_all(&tmp).map_err(io_err(&tmp))?;
    let runner = LilyPond {
        lilypond: "lilypond".into(),
        convert_ly: "convert-ly".into(),
        settings: root.join("assets/lilypond/check-settings.ly"),
        tmp: tmp.clone(),
        serial: AtomicUsize::new(0),
    };
    let started = Instant::now();
    let entries = filter(&runner, &raw, &corpus, jobs, timeout).map_err(io_err(&raw))?;
    let _ = fs::remove_dir_all(&tmp);

    let manifest = root.join("bench/results/corpus-manifest.json");
    fs::create_dir_all(manifest.parent().expect("has a folder")).map_err(io_err(&manifest))?;
    fs::write(
        &manifest,
        manifest_json(&entries, &commit, timeout.as_secs()),
    )
    .map_err(io_err(&manifest))?;
    println!(
        "{} files in {:.0} s with {jobs} jobs; manifest at bench/results/corpus-manifest.json",
        entries.len(),
        started.elapsed().as_secs_f64()
    );
    // Entries are in path order, and a path starts with its source, so each source is one run.
    let mut sources: Vec<&str> = entries.iter().map(|e| e.source.as_str()).collect();
    sources.dedup();
    for source in sources {
        let n = |c: Class| {
            entries
                .iter()
                .filter(|e| e.source == source && e.class == c)
                .count()
        };
        println!(
            "{source}: accepted {}, converted {}, rejected {}, timeout {}",
            n(Class::Accepted),
            n(Class::Converted),
            n(Class::Rejected),
            n(Class::Timeout)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    const LIMIT: Duration = Duration::from_secs(60);

    /// A scratch folder for one test, emptied first so that an earlier failed run leaves nothing in it.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swara-corpus-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    fn replace(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
        let at = bytes.windows(from.len()).position(|w| w == from).unwrap();
        [&bytes[..at], to, &bytes[at + from.len()..]].concat()
    }

    type Rule = Box<dyn Fn(&[u8]) -> Outcome + Sync>;
    type Rewrite = Box<dyn Fn(&[u8]) -> Option<Vec<u8>> + Sync>;

    /// A Runner that decides from the file's bytes, and records every call with the folder it ran in.
    struct Fake {
        check: Rule,
        convert: Rewrite,
        calls: Mutex<Vec<(&'static str, PathBuf, PathBuf)>>,
    }

    impl Fake {
        fn new(
            check: impl Fn(&[u8]) -> Outcome + Sync + 'static,
            convert: impl Fn(&[u8]) -> Option<Vec<u8>> + Sync + 'static,
        ) -> Fake {
            Fake {
                check: Box::new(check),
                convert: Box::new(convert),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn converts(&self) -> usize {
            let calls = self.calls.lock().unwrap();
            calls.iter().filter(|(what, ..)| *what == "convert").count()
        }
    }

    impl Runner for Fake {
        fn run(&self, file: &Path, cwd: &Path, _timeout: Duration) -> Outcome {
            let calls = &mut self.calls.lock().unwrap();
            calls.push(("run", file.to_path_buf(), cwd.to_path_buf()));
            (self.check)(&fs::read(file).unwrap())
        }

        fn convert(&self, file: &Path, cwd: &Path, _timeout: Duration) -> Outcome {
            let calls = &mut self.calls.lock().unwrap();
            calls.push(("convert", file.to_path_buf(), cwd.to_path_buf()));
            match (self.convert)(&fs::read(file).unwrap()) {
                Some(new) => {
                    fs::write(file, new).unwrap();
                    Outcome::Passed
                }
                None => Outcome::Failed,
            }
        }
    }

    /// Passes only text that names LilyPond 2.26; conversion rewrites 2.18.2 to 2.26.0.
    fn needs_conversion() -> Fake {
        Fake::new(
            |b| match contains(b, b"2.26.0") {
                true => Outcome::Passed,
                false => Outcome::Failed,
            },
            |b| Some(replace(b, b"2.18.2", b"2.26.0")),
        )
    }

    #[test]
    fn accepted_when_exit_zero() {
        let dir = scratch("accepted");
        put(&dir.join("raw"), "regression/a.ly", b"{ c'4 }\n");
        let fake = Fake::new(|_| Outcome::Passed, |_| None);
        let entries = filter(&fake, &dir.join("raw"), &dir.join("out"), 2, LIMIT).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "regression/a.ly");
        assert_eq!(entries[0].source, "regression");
        assert_eq!(entries[0].class, Class::Accepted);
        assert_eq!(
            fs::read(dir.join("out/accepted/regression/a.ly")).unwrap(),
            b"{ c'4 }\n"
        );
        assert_eq!(fake.converts(), 0);
    }

    #[test]
    fn converted_when_convert_ly_fixes_it() {
        let dir = scratch("converted");
        let raw = dir.join("raw");
        let original = b"\\version \"2.18.2\"\n{ c'4 }\n";
        put(&raw, "mutopia/BachJS/p/p.ly", original);
        let entries = filter(&needs_conversion(), &raw, &dir.join("out"), 2, LIMIT).unwrap();
        let e = &entries[0];
        assert_eq!(e.class, Class::Converted);
        assert_eq!(
            e.sha256,
            sha256::hex(original),
            "the hash is of the original bytes"
        );
        let stored = fs::read(dir.join("out/accepted/mutopia/BachJS/p/p.ly")).unwrap();
        assert!(
            contains(&stored, b"2.26.0"),
            "the accepted copy is the converted text"
        );
        assert_eq!(e.converted_sha256, Some(sha256::hex(&stored)));
        assert_eq!(
            fs::read(raw.join("mutopia/BachJS/p/p.ly")).unwrap(),
            original
        );
        let left: Vec<_> = fs::read_dir(raw.join("mutopia/BachJS/p"))
            .unwrap()
            .collect();
        assert_eq!(left.len(), 1, "the working copy is removed");
    }

    #[test]
    fn timeout_is_its_own_class() {
        let dir = scratch("timeout");
        put(&dir.join("raw"), "snippets/slow.ly", b"{ c'4 }\n");
        let fake = Fake::new(|_| Outcome::TimedOut, |b| Some(b.to_vec()));
        let entries = filter(&fake, &dir.join("raw"), &dir.join("out"), 2, LIMIT).unwrap();
        assert_eq!(entries[0].class, Class::Timeout);
        assert!(dir.join("out/timeout/snippets/slow.ly").is_file());
        assert_eq!(
            fake.converts(),
            0,
            "a file that ran out of time is not run again"
        );
    }

    #[test]
    fn non_utf8_file_is_classified_not_skipped() {
        let dir = scratch("latin1");
        let latin1 = b"\\header { composer = \"Ren\xE9\" }\n{ c'4 }\n";
        put(&dir.join("raw"), "mutopia/X/w/w.ly", latin1);
        put(&dir.join("raw"), "mutopia/X/w/ok.ly", b"{ d'4 }\n");
        let fake = Fake::new(|_| Outcome::Passed, |_| None);
        let entries = filter(&fake, &dir.join("raw"), &dir.join("out"), 2, LIMIT).unwrap();
        assert_eq!(entries.len(), 2);
        let e = entries
            .iter()
            .find(|e| e.path == "mutopia/X/w/w.ly")
            .unwrap();
        assert_eq!(e.class, Class::Accepted);
        assert_eq!(e.bytes, latin1.len() as u64);
        assert_eq!(
            fs::read(dir.join("out/accepted/mutopia/X/w/w.ly")).unwrap(),
            latin1
        );
    }

    #[test]
    fn runs_from_the_files_own_folder() {
        let dir = scratch("folder");
        let raw = dir.join("raw");
        put(&raw, "mutopia/A/b/c/piece.ly", b"\\version \"2.18.2\"\n");
        put(&raw, "mutopia/A/b/common.ily", b"x = { c'4 }\n");
        let fake = needs_conversion();
        let entries = filter(&fake, &raw, &dir.join("out"), 2, LIMIT).unwrap();
        assert_eq!(
            entries.len(),
            1,
            "an .ily file is include context, not a corpus entry"
        );
        let calls = fake.calls.lock().unwrap();
        let what: Vec<&str> = calls.iter().map(|(w, ..)| *w).collect();
        assert_eq!(what, ["run", "convert", "run"]);
        for (_, file, cwd) in calls.iter() {
            assert!(file.is_absolute(), "{file:?}");
            assert_eq!(file.parent(), Some(cwd.as_path()));
        }
    }

    #[test]
    fn manifest_names_each_file_and_counts_per_source() {
        let e = |path: &str, class| Entry {
            path: path.to_string(),
            source: path.split('/').next().unwrap().to_string(),
            bytes: 9,
            sha256: "ab".repeat(32),
            class,
            converted_sha256: None,
        };
        let entries = [
            e("regression/a.ly", Class::Accepted),
            e("regression/b \"q\".ly", Class::Rejected),
            e("mutopia/X/c.ly", Class::Timeout),
        ];
        let json = manifest_json(&entries, "abc123", 60);
        assert!(json.contains(r#""mutopia_commit": "abc123""#), "{json}");
        assert!(
            json.contains(r#""path": "regression/b \"q\".ly""#),
            "{json}"
        );
        assert!(json.contains(r#""class": "timeout""#), "{json}");
        assert!(
            json.contains(
                r#""regression": {"accepted": 1, "converted": 0, "rejected": 1, "timeout": 0}"#
            ),
            "{json}"
        );
    }

    #[test]
    fn mutopia_urls_must_sit_under_the_pinned_ftp_tree() {
        let base = "https://raw.githubusercontent.com/MutopiaProject/MutopiaProject";
        let sha = "2144afd6f52d56c5b6995b8b589ef1268b3139f0";
        let good = format!("{base}/{sha}/ftp/BachJS/BWV846/wtk1-prelude1/wtk1-prelude1.ly");
        assert_eq!(
            mutopia_dest(&good),
            Some((
                sha.to_string(),
                PathBuf::from("BachJS/BWV846/wtk1-prelude1/wtk1-prelude1.ly")
            ))
        );
        assert_eq!(
            mutopia_dest(&format!("{base}/{sha}/ftp/A/../../x.ly")),
            None
        );
        assert_eq!(mutopia_dest(&format!("{base}/main/ftp/A/b.ly")), None);
        assert_eq!(
            mutopia_dest(&format!("https://example.com/{sha}/ftp/A/b.ly")),
            None
        );
    }

    /// The real runner, with stand-in programs for LilyPond: exit status decides, and a run past its
    /// limit is killed rather than waited for.
    #[test]
    fn lilypond_runner_reads_exit_status_and_kills_at_the_limit() {
        let dir = scratch("runner");
        let file = dir.join("a.ly");
        fs::write(&file, b"{ c'4 }\n").unwrap();
        let sleeper = dir.join("sleeper");
        fs::write(&sleeper, "#!/bin/sh\nsleep 5\n").unwrap();
        std::process::Command::new("chmod")
            .args(["+x"])
            .arg(&sleeper)
            .status()
            .unwrap();
        let runner = |program: &Path| LilyPond {
            lilypond: program.to_path_buf(),
            convert_ly: program.to_path_buf(),
            settings: dir.join("settings.ly"),
            tmp: dir.join("tmp"),
            serial: Default::default(),
        };
        let pass = runner(Path::new("/usr/bin/true"));
        assert_eq!(pass.run(&file, &dir, LIMIT), Outcome::Passed);
        let fail = runner(Path::new("/usr/bin/false"));
        assert_eq!(fail.run(&file, &dir, LIMIT), Outcome::Failed);
        assert_eq!(fail.convert(&file, &dir, LIMIT), Outcome::Failed);
        let started = Instant::now();
        let slow = runner(&sleeper).run(&file, &dir, Duration::from_millis(200));
        assert_eq!(slow, Outcome::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
