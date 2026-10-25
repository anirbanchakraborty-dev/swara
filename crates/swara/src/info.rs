//! `swara info`: what the plugin and `:checkhealth swara` need to know about this installation.

use crate::commanddata::cache;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "macos")]
const LIBNAME: &str = "libswara_parser.dylib";
#[cfg(windows)]
const LIBNAME: &str = "swara_parser.dll";
#[cfg(not(any(target_os = "macos", windows)))]
const LIBNAME: &str = "libswara_parser.so";

/// The grammar commit this build compiled in, from the lockfile (see `build.rs`).
const GRAMMAR_REV: &str = env!("SWARA_GRAMMAR_REV");

pub fn info() -> Value {
    let lilypond = Lilypond::find();
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "grammar": grammar_library(),
        "grammar_symbol": "lilypond_swara",
        "grammar_rev": GRAMMAR_REV,
        "abi": tree_sitter::Language::new(tree_sitter_lilypond::LANGUAGE).abi_version(),
        "lilypond": lilypond.as_ref().map(|l| json!({ "path": l.path, "version": l.version })),
        "cache": lilypond.as_ref().map(|l| {
            let path = cache::path(&l.path, &l.version);
            json!({ "present": path.is_file(), "path": path })
        }),
    })
}

/// The grammar library installed beside this program: `<exe dir>/../lib/<libname>` for a Homebrew
/// or release layout, then `<exe dir>/<libname>` for a Cargo build. The program's own path is
/// canonicalised first, so a symlink on `PATH` still leads to the real installation.
fn grammar_library() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let dir = exe.parent()?;
    [dir.join("..").join("lib").join(LIBNAME), dir.join(LIBNAME)]
        .into_iter()
        .find(|candidate| candidate.is_file())
        .and_then(|found| found.canonicalize().ok())
}

pub struct Lilypond {
    /// Canonicalised, so two links to one installation share a cache.
    pub path: PathBuf,
    pub version: String,
}

impl Lilypond {
    /// The first `lilypond` on `PATH` that runs and reports a version.
    pub fn find() -> Option<Lilypond> {
        Self::at(&find_on_path("lilypond")?)
    }

    /// The LilyPond at `path`, when it runs and reports a version.
    pub fn at(path: &Path) -> Option<Lilypond> {
        let path = path.canonicalize().ok()?;
        let version = version_of(&path)?;
        Some(Lilypond { path, version })
    }
}

pub(crate) fn find_on_path(name: &str) -> Option<PathBuf> {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(&file))
        .find(|candidate| candidate.is_file())
}

fn version_of(lilypond: &Path) -> Option<String> {
    let out = Command::new(lilypond).arg("--version").env("LC_ALL", "C").output().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    parse_version(stdout.lines().next()?)
}

/// The version from the first line of `lilypond --version`, such as
/// `GNU LilyPond 2.26.0 (running Guile 3.0)`.
fn parse_version(first_line: &str) -> Option<String> {
    let mut words = first_line.split_whitespace();
    words.find(|word| *word == "LilyPond")?;
    let version = words.next()?;
    version.starts_with(|c: char| c.is_ascii_digit()).then(|| version.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_from_first_line() {
        assert_eq!(
            parse_version("GNU LilyPond 2.26.0 (running Guile 3.0)").as_deref(),
            Some("2.26.0")
        );
        assert_eq!(parse_version("GNU LilyPond 2.24.4").as_deref(), Some("2.24.4"));
        assert_eq!(parse_version("lilypond: command not found"), None);
        assert_eq!(parse_version("GNU LilyPond"), None);
    }
}
