//! `cargo xtask dist` for the machine running the test: the archive unpacks into a working
//! installation, and `SHA256SUMS` names it with its real checksum.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A folder under the system's temporary folder, removed when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("swara-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn host_triple() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        other => panic!("no release triple for {other:?}"),
    }
}

fn swara_version() -> String {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/swara/Cargo.toml");
    let text = std::fs::read_to_string(manifest).unwrap();
    text.lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("crates/swara/Cargo.toml names a version")
        .to_owned()
}

/// The SHA-256 of `path` from the system's own tool, so the check does not trust the code it tests.
fn system_sha256(path: &Path) -> String {
    let out = Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .or_else(|_| Command::new("sha256sum").arg(path).output())
        .expect("shasum or sha256sum runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()[..64].to_owned()
}

#[test]
fn dist_layout() {
    let triple = host_triple();
    let version = swara_version();
    let out = TempDir::new("dist-out");
    let unpacked = TempDir::new("dist-unpacked");

    // Twice, because a second run must replace the archive's line in `SHA256SUMS`, not add one.
    for _ in 0..2 {
        let dist = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(["dist", "--target", triple, "--out"])
            .arg(&out.0)
            .output()
            .unwrap();
        assert!(
            dist.status.success(),
            "{}",
            String::from_utf8_lossy(&dist.stderr)
        );
    }

    let name = format!("swara-{version}-{triple}");
    let (archive_name, exe, lib) = if cfg!(windows) {
        (
            format!("{name}.zip"),
            "bin/swara.exe",
            "bin/swara_parser.dll",
        )
    } else if cfg!(target_os = "macos") {
        (
            format!("{name}.tar.gz"),
            "bin/swara",
            "lib/libswara_parser.dylib",
        )
    } else {
        (
            format!("{name}.tar.gz"),
            "bin/swara",
            "lib/libswara_parser.so",
        )
    };
    let archive = out.0.join(&archive_name);
    assert!(archive.is_file(), "no {archive_name} in the output folder");

    let tar = Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(&unpacked.0)
        .status()
        .unwrap();
    assert!(tar.success());
    let root = unpacked.0.join(&name);
    let info = Command::new(root.join(exe)).arg("info").output().unwrap();
    assert!(
        info.status.success(),
        "{}",
        String::from_utf8_lossy(&info.stderr)
    );
    let info: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(info["version"], version.as_str());
    let grammar = PathBuf::from(info["grammar"].as_str().expect("info names the grammar"));
    assert_eq!(grammar, root.join(lib).canonicalize().unwrap());

    let sums = std::fs::read_to_string(out.0.join("SHA256SUMS")).unwrap();
    let line = format!("{}  {archive_name}", system_sha256(&archive));
    assert_eq!(
        sums.lines()
            .filter(|l| l.ends_with(&archive_name))
            .collect::<Vec<_>>(),
        [line.as_str()],
        "SHA256SUMS:\n{sums}"
    );
}
