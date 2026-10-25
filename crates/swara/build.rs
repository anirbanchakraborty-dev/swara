//! Records which grammar commit this build compiles in, for `swara info` and `:checkhealth`.

use std::path::Path;
use std::process::Command;

fn main() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lock = crate_dir.join("../../Cargo.lock");
    let manifest = crate_dir.join("Cargo.toml");
    println!("cargo::rerun-if-changed={}", lock.display());
    println!("cargo::rerun-if-changed={}", manifest.display());
    let lock_text = std::fs::read_to_string(&lock).unwrap_or_default();
    let rev = locked_rev(&lock_text)
        .or_else(|| {
            let manifest_text = std::fs::read_to_string(&manifest).ok()?;
            checkout_rev(&crate_dir.join(grammar_path(&manifest_text)?))
        })
        .unwrap_or_else(|| "local".to_owned());
    println!("cargo::rustc-env=SWARA_GRAMMAR_REV={rev}");
}

/// The commit after `#` in the grammar package's `source` line. A path dependency has no source
/// line.
fn locked_rev(lock: &str) -> Option<String> {
    lock.split("[[package]]")
        .find(|package| {
            package
                .lines()
                .any(|line| line.trim() == r#"name = "tree-sitter-lilypond""#)
        })
        .and_then(|package| {
            package
                .lines()
                .find_map(|line| line.trim().strip_prefix("source = \"git+"))
        })
        .and_then(|source| source.trim_end_matches('"').rsplit_once('#'))
        .map(|(_, rev)| rev.to_owned())
}

/// The folder this crate's manifest names for the grammar when it is a path dependency.
fn grammar_path(manifest: &str) -> Option<&str> {
    manifest
        .lines()
        .find(|line| line.trim_start().starts_with("tree-sitter-lilypond"))
        .and_then(|line| line.split_once("path = \""))
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(path, _)| path)
}

/// The commit a git checkout of the grammar is on, or `None` when the folder is not a git checkout
/// or git is missing. The files that change when the checkout moves are marked for Cargo, so a new
/// commit rebuilds this crate.
fn checkout_rev(dir: &Path) -> Option<String> {
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .ok()?;
        let text = String::from_utf8(out.stdout).ok()?;
        out.status.success().then(|| text.trim().to_owned())
    };
    let rev = git(&["rev-parse", "HEAD"])?;
    let mut watched = vec![git(&["rev-parse", "--git-path", "HEAD"])?];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.extend(git(&["rev-parse", "--git-path", &branch]));
    }
    for path in watched.iter().map(|path| dir.join(path)) {
        if path.is_file() {
            println!("cargo::rerun-if-changed={}", path.display());
        }
    }
    Some(rev)
}
