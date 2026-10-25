//! Where the command data for one LilyPond installation is cached.

use std::path::{Path, PathBuf};

/// `<cache dir>/swara/commands-<key>.jsonl`, where the key is FNV-1a 64 over the version and the
/// canonical path, so two versions, or two installations of one version, never share a cache.
pub fn path(lilypond_canonical: &Path, version: &str) -> PathBuf {
    let mut key = version.as_bytes().to_vec();
    key.push(0);
    key.extend_from_slice(lilypond_canonical.as_os_str().as_encoded_bytes());
    cache_dir()
        .join("swara")
        .join(format!("commands-{:016x}.jsonl", fnv1a64(&key)))
}

fn cache_dir() -> PathBuf {
    let var = |name| std::env::var_os(name).map(PathBuf::from).filter(|p| p.is_absolute());
    let home = || var("HOME");
    let dir = if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library").join("Caches"))
    } else if cfg!(windows) {
        var("LOCALAPPDATA")
    } else {
        var("XDG_CACHE_HOME").or_else(|| home().map(|h| h.join(".cache")))
    };
    dir.unwrap_or_else(std::env::temp_dir)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_known_values() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn cache_key_differs_by_version() {
        let lilypond = Path::new("/opt/homebrew/bin/lilypond");
        let old = path(lilypond, "2.24.4");
        let new = path(lilypond, "2.26.0");
        assert_ne!(old, new);
        assert_eq!(new, path(lilypond, "2.26.0"));
        let name = new.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("commands-") && name.ends_with(".jsonl"), "{name}");
        assert_eq!(name.len(), "commands-".len() + 16 + ".jsonl".len(), "{name}");
        assert_eq!(new.parent().unwrap().file_name().unwrap(), "swara");
    }

    #[test]
    fn cache_key_differs_by_path() {
        assert_ne!(
            path(Path::new("/usr/local/bin/lilypond"), "2.26.0"),
            path(Path::new("/opt/homebrew/bin/lilypond"), "2.26.0")
        );
    }
}
