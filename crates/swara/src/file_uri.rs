//! Converting between `file:` URIs and paths. A URI is written the way Neovim writes one, because
//! URIs are compared as strings, and a file must get the same URI from both.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use lsp_types::Uri;

/// The bytes Neovim leaves as they are in a path. Every other byte is written `%XX`.
const SAFE: &[u8] = b"-._~!$&'()*+,;=:@/";

/// The path a `file:` URI names, or `None` for any other scheme, another host, or a name that is
/// not UTF-8.
pub fn to_path(uri: &Uri) -> Option<PathBuf> {
    let rest = uri.as_str().strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    // Anything else before the path names another machine.
    if !rest.starts_with('/') {
        return None;
    }
    let path = String::from_utf8(percent_decode(rest.as_bytes())?).ok()?;
    if cfg!(windows) {
        // `/C:/x` is `C:\x`.
        let drive = path.len() > 2 && path.as_bytes()[2] == b':';
        let path = if drive { &path[1..] } else { &path[..] };
        return Some(PathBuf::from(path.replace('/', "\\")));
    }
    Some(PathBuf::from(path))
}

/// The `file:` URI for an absolute path, or `None` for a relative path or one that is not UTF-8.
pub fn from_path(path: &Path) -> Option<Uri> {
    if !path.is_absolute() {
        return None;
    }
    let path = path.to_str()?;
    let path = if cfg!(windows) {
        format!("/{}", path.replace('\\', "/"))
    } else {
        path.to_owned()
    };
    let mut uri = String::from("file://");
    for &byte in path.as_bytes() {
        if byte.is_ascii_alphanumeric() || SAFE.contains(&byte) {
            uri.push(byte as char);
        } else {
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri.parse().ok()
}

fn percent_decode(text: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            i += 3;
        } else {
            out.push(text[i]);
            i += 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(text: &str) -> Uri {
        text.parse().unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn spaces_and_bengali_round_trip() {
        let path = Path::new("/Users/x/Music/আমার গান/lesson 1.ly");
        let made = from_path(path).unwrap();
        assert_eq!(
            made.as_str(),
            "file:///Users/x/Music/%E0%A6%86%E0%A6%AE%E0%A6%BE%E0%A6%B0%20%E0%A6%97%E0%A6%BE%E0%A6%A8/lesson%201.ly"
        );
        assert_eq!(to_path(&made).as_deref(), Some(path));
    }

    #[cfg(unix)]
    #[test]
    fn marks_neovim_leaves_alone() {
        let path = Path::new("/tmp/a-b_c.d~e!$&'()*+,;=:@/f#g%h?.ly");
        let made = from_path(path).unwrap();
        assert_eq!(
            made.as_str(),
            "file:///tmp/a-b_c.d~e!$&'()*+,;=:@/f%23g%25h%3F.ly"
        );
        assert_eq!(to_path(&made).as_deref(), Some(path));
    }

    #[cfg(unix)]
    #[test]
    fn localhost_and_lowercase_escapes() {
        assert_eq!(
            to_path(&uri("file://localhost/tmp/a.ly")),
            Some(PathBuf::from("/tmp/a.ly"))
        );
        assert_eq!(
            to_path(&uri("file:///tmp/a%c3%a9.ly")),
            Some(PathBuf::from("/tmp/aé.ly"))
        );
    }

    #[test]
    fn other_schemes_and_hosts() {
        assert_eq!(to_path(&uri("untitled:Untitled-1")), None);
        assert_eq!(to_path(&uri("file://server/share/a.ly")), None);
        assert_eq!(to_path(&uri("file:///tmp/%FF.ly")), None);
        assert_eq!(from_path(Path::new("relative/a.ly")), None);
    }
}
