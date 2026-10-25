//! `cargo xtask dist --target <triple> --out <dir>`: builds `swara` and its grammar library in
//! release mode, packs them as the plugin's download expects, and records the archive's checksum
//! in `<dir>/SHA256SUMS`.

use crate::sha256;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The platforms a release carries an archive for.
pub const TRIPLES: [&str; 5] = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
];

pub fn run(root: &Path, triple: &str, out: &Path) -> Result<(), String> {
    if !TRIPLES.contains(&triple) {
        return Err(format!(
            "{triple} is not a release triple; use one of {}",
            TRIPLES.join(", ")
        ));
    }
    let version = version(root)?;
    build(root, triple)?;

    let windows = triple.contains("windows");
    // `swara info` looks for the library in `../lib` beside the program, then beside it.
    let (exe, lib, lib_dir) = if windows {
        ("swara.exe", "swara_parser.dll", "bin")
    } else if triple.contains("apple") {
        ("swara", "libswara_parser.dylib", "lib")
    } else {
        ("swara", "libswara_parser.so", "lib")
    };
    let target = target_dir(root);
    let built = target.join(triple).join("release");
    let name = format!("swara-{version}-{triple}");
    let staging = target.join("dist").join(triple);
    let top = staging.join(&name);
    let _ = fs::remove_dir_all(&staging);
    for (file, dir) in [(exe, "bin"), (lib, lib_dir)] {
        fs::create_dir_all(top.join(dir)).map_err(|e| format!("{}: {e}", top.display()))?;
        fs::copy(built.join(file), top.join(dir).join(file))
            .map_err(|e| format!("{}: {e}", built.join(file).display()))?;
    }

    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let out = std::path::absolute(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let archive_name = format!("{name}.{}", if windows { "zip" } else { "tar.gz" });
    let archive = out.join(&archive_name);
    let _ = fs::remove_file(&archive);
    let mut tar = Command::new("tar");
    // Keeps macOS from adding `._` resource-fork entries.
    tar.env("COPYFILE_DISABLE", "1");
    tar.args(if windows {
        ["-a", "-cf"]
    } else {
        ["-cz", "-f"]
    });
    tar.arg(&archive).arg("-C").arg(&staging).arg(&name);
    let status = tar.status().map_err(|e| format!("tar: {e}"))?;
    let _ = fs::remove_dir_all(&staging);
    if !status.success() {
        return Err(format!("tar could not write {}", archive.display()));
    }

    let bytes = fs::read(&archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    record_sum(&out.join("SHA256SUMS"), &archive_name, &sha256::hex(&bytes))?;
    println!("{}", archive.display());
    Ok(())
}

/// The version in `crates/swara/Cargo.toml`, which names the archive.
fn version(root: &Path) -> Result<String, String> {
    let manifest = root.join("crates/swara/Cargo.toml");
    let text = fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    text.lines()
        .find_map(|line| line.strip_prefix("version = \"")?.strip_suffix('"'))
        .map(str::to_owned)
        .ok_or_else(|| format!("no version in {}", manifest.display()))
}

fn build(root: &Path, triple: &str) -> Result<(), String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(root)
        .args(["build", "--release", "--locked", "--target", triple])
        .args(["-p", "swara", "-p", "swara-parser"])
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("the release build for {triple} failed"))
    }
}

fn target_dir(root: &Path) -> PathBuf {
    // Joining an absolute path replaces `root`, so this covers both kinds of setting.
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), |dir| root.join(dir))
}

/// Writes `<sum>  <file>` into `SHA256SUMS`, replacing any earlier line for the same file.
fn record_sum(path: &Path, file: &str, sum: &str) -> Result<(), String> {
    let existing = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let mut lines: Vec<String> = existing
        .lines()
        .filter(|line| line.split_once("  ").map(|(_, name)| name) != Some(file))
        .map(str::to_owned)
        .collect();
    lines.push(format!("{sum}  {file}"));
    fs::write(path, lines.join("\n") + "\n").map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::record_sum;

    #[test]
    fn record_sum_replaces_only_its_own_line() {
        let dir = std::env::temp_dir().join(format!("swara-sums-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("SHA256SUMS");
        std::fs::write(&path, "aa  one.tar.gz\nbb  two.tar.gz\n").unwrap();
        record_sum(&path, "one.tar.gz", "cc").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(text, "bb  two.tar.gz\ncc  one.tar.gz\n");
    }
}
