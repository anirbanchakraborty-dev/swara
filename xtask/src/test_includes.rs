//! `cargo xtask test-includes`: the tests of the LilyPond include files, which run inside LilyPond.
//!
//! Each `test/<folder>/<name>.ly` reports through `ly:warning`: a `swara-test: mismatch` line for
//! each check that failed, and a closing `swara-test: N passed` line. Files whose names start with
//! `sample-` are engraved for a person to read and are not run here.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Runs every test file under `test/` through LilyPond and fails when any check failed, when
/// LilyPond failed, or when a file did not report its total.
pub fn run(root: &Path) -> Result<(), String> {
    let files = test_files(&root.join("test"))?;
    if files.is_empty() {
        return Err(format!(
            "no test files under {}",
            root.join("test").display()
        ));
    }
    let mut failed = Vec::new();
    for file in &files {
        let output = Command::new("lilypond")
            .env("LC_ALL", "C")
            .args(["-l", "WARNING", "-dno-print-pages"])
            .arg(file)
            .output()
            .map_err(|e| format!("could not run lilypond: {e}"))?;
        let name = file.strip_prefix(root).unwrap_or(file).display();
        match verdict(
            &String::from_utf8_lossy(&output.stderr),
            output.status.success(),
        ) {
            Ok(passed) => println!("{name}: {passed} passed"),
            Err(why) => failed.push(format!("{name}: {why}")),
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("\n"))
    }
}

/// Every `<dir>/<folder>/<name>.ly` whose name does not start with `sample-`, in path order.
fn test_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = |dir: &Path| -> Result<Vec<PathBuf>, String> {
        let read =
            fs::read_dir(dir).map_err(|e| format!("could not read {}: {e}", dir.display()))?;
        read.map(|entry| {
            entry
                .map(|e| e.path())
                .map_err(|e| format!("could not read {}: {e}", dir.display()))
        })
        .collect()
    };
    let mut files = Vec::new();
    for folder in entries(dir)?.into_iter().filter(|path| path.is_dir()) {
        files.extend(entries(&folder)?.into_iter().filter(|path| {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            path.extension().is_some_and(|e| e == "ly") && !name.starts_with("sample-")
        }));
    }
    files.sort();
    Ok(files)
}

/// What one file's run says: the number of checks that passed, or why the file failed. LilyPond
/// puts `warning: ` before each line that `ly:warning` printed, so that is set aside first.
fn verdict(stderr: &str, succeeded: bool) -> Result<u32, String> {
    let lines: Vec<&str> = stderr
        .lines()
        .map(|line| line.strip_prefix("warning: ").unwrap_or(line))
        .collect();
    let mismatches: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| line.starts_with("swara-test: mismatch"))
        .collect();
    if !mismatches.is_empty() {
        return Err(mismatches.join("\n"));
    }
    if !succeeded {
        return Err(format!("lilypond failed\n{}", stderr.trim_end()));
    }
    let passed = lines.iter().rev().find_map(|line| {
        line.strip_prefix("swara-test: ")?
            .strip_suffix(" passed")?
            .parse::<u32>()
            .ok()
    });
    match passed {
        Some(0) => Err("no checks ran".to_string()),
        Some(n) => Ok(n),
        None => Err(format!(
            "no `swara-test: N passed` line\n{}",
            stderr.trim_end()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{test_files, verdict};

    #[test]
    fn counts_the_closing_line() {
        assert_eq!(verdict("warning: swara-test: 12 passed\n", true), Ok(12));
    }

    #[test]
    fn a_mismatch_fails_and_is_named() {
        let stderr =
            "warning: swara-test: mismatch: kta gave \"x\"\nwarning: swara-test: 11 passed\n";
        let error = verdict(stderr, true).unwrap_err();
        assert!(error.contains("mismatch: kta"), "{error}");
    }

    #[test]
    fn a_missing_closing_line_fails() {
        let error = verdict("warning: something else\n", true).unwrap_err();
        assert!(error.contains("swara-test: N passed"), "{error}");
    }

    #[test]
    fn a_failed_run_fails() {
        let stderr = "error: Guile signaled an error\nwarning: swara-test: 3 passed\n";
        let error = verdict(stderr, false).unwrap_err();
        assert!(error.contains("Guile signaled"), "{error}");
    }

    #[test]
    fn no_checks_at_all_fails() {
        assert!(verdict("warning: swara-test: 0 passed\n", true).is_err());
    }

    #[test]
    fn only_test_files_are_run() {
        let dir = std::env::temp_dir().join(format!("swara-test-includes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for file in [
            "b/two.ly",
            "a/one.ly",
            "a/sample-sheet.ly",
            "a/table.tsv",
            "a/lib.ily",
        ] {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        std::fs::write(dir.join("top.ly"), "").unwrap();
        let files = test_files(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(files.unwrap(), [dir.join("a/one.ly"), dir.join("b/two.ly")]);
    }
}
