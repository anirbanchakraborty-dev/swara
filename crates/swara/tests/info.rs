//! `swara info` and the command line around it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "macos")]
const LIBNAME: &str = "libswara_parser.dylib";
#[cfg(windows)]
const LIBNAME: &str = "swara_parser.dll";
#[cfg(not(any(target_os = "macos", windows)))]
const LIBNAME: &str = "libswara_parser.so";

fn swara() -> Command {
    Command::new(env!("CARGO_BIN_EXE_swara"))
}

fn info(mut command: Command) -> serde_json::Value {
    let out = command.arg("info").output().expect("swara runs");
    assert!(
        out.status.success(),
        "swara info failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("swara info prints one JSON object")
}

/// The grammar library Cargo built for these tests. A dependency's library stays in
/// `target/<profile>/deps`, beside this test program, and `cargo build` also copies it one folder up.
fn built_library() -> PathBuf {
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    [deps.join(LIBNAME), deps.parent().unwrap().join(LIBNAME)]
        .into_iter()
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| panic!("{LIBNAME} was not built beside {}", deps.display()))
}

/// A fresh folder under the system's temporary folder, unique to this test run.
fn scratch(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("swara-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// The layout of a Cargo build or an unpacked archive: the library beside the program.
#[test]
fn info_reports_grammar_and_abi() {
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_swara"));
    let root = scratch("info-flat");
    let copy = root.join(exe.file_name().unwrap());
    fs::copy(&exe, &copy).unwrap();
    fs::copy(built_library(), root.join(LIBNAME)).unwrap();

    let v = info(Command::new(&copy));
    let expected = root.canonicalize().unwrap().join(LIBNAME);
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(v["abi"], 15);
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(v["grammar_symbol"], "lilypond_swara");
    assert!(
        v["grammar_rev"].as_str().is_some_and(|rev| !rev.is_empty()),
        "grammar_rev names the grammar commit: {v}"
    );
    let grammar = v["grammar"]
        .as_str()
        .unwrap_or_else(|| panic!("no grammar library found: {v}"));
    assert_eq!(Path::new(grammar), expected);
}

/// The Homebrew layout: `bin/swara` reached through a symlink, and the library in `lib`.
#[cfg(unix)]
#[test]
fn info_through_symlink() {
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_swara"));
    let lib = built_library();

    let root = scratch("info-symlink");
    let prefix = root.join("prefix");
    fs::create_dir_all(prefix.join("bin")).unwrap();
    fs::create_dir_all(prefix.join("lib")).unwrap();
    fs::copy(&exe, prefix.join("bin").join("swara")).unwrap();
    fs::copy(&lib, prefix.join("lib").join(LIBNAME)).unwrap();
    let links = root.join("links");
    fs::create_dir_all(&links).unwrap();
    std::os::unix::fs::symlink(prefix.join("bin").join("swara"), links.join("swara")).unwrap();

    let v = info(Command::new(links.join("swara")));
    let grammar = PathBuf::from(v["grammar"].as_str().expect("a grammar library is found"));
    let expected = prefix.canonicalize().unwrap().join("lib").join(LIBNAME);
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(grammar, expected);
}

#[test]
fn unknown_subcommand_exits_2() {
    let out = swara().arg("frobnicate").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("usage: swara"), "{stderr}");
    assert!(out.stdout.is_empty());
}

#[test]
fn version_flag_prints_version() {
    let out = swara().arg("--version").output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("swara {}", env!("CARGO_PKG_VERSION"))
    );
}
