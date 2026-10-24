//! The fixed 100-bar song that every speed test in this repository compiles.

use std::fmt::Write;

/// Sixteen eighth notes over two bars, all inside the guitar's range.
const PATTERN: [&str; 2] = ["c'8 d' e' f' g' a' b' c''", "b'8 a' g' f' e' d' c' g"];

/// Writes a song of `bars` bars: a melody on a treble_8 staff, a tab staff, two note-name lines
/// and one lyric syllable per note. The output depends only on `bars`.
pub fn generate_song(bars: u32) -> String {
    let mut s = String::new();
    s.push_str("\\version \"2.26.0\"\n\\language \"english\"\n\n");
    s.push_str("mus = {\n  \\time 4/4 \\key c \\major\n");
    for bar in 0..bars as usize {
        writeln!(s, "  {} |", PATTERN[bar % PATTERN.len()]).unwrap();
    }
    s.push_str("}\n\nwords = \\lyricmode {\n");
    for _ in 0..bars {
        s.push_str("  la la la la la la la la\n");
    }
    s.push_str("}\n\n");
    s.push_str(concat!(
        "\\score {\n",
        "  <<\n",
        "    \\new Staff \\new Voice = \"melody\" { \\clef \"treble_8\" \\mus }\n",
        "    \\new NoteNames \\mus\n",
        "    \\new Lyrics \\lyricsto \"melody\" { \\words }\n",
        "    \\new TabStaff { \\set TabStaff.stringTunings = \\stringTuning <e, a, d g b e'> \\mus }\n",
        "    \\new NoteNames \\mus\n",
        "  >>\n",
        "  \\layout { }\n",
        "  \\midi { }\n",
        "}\n",
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn song_has_requested_bars_and_parts() {
        let s = generate_song(100);
        assert!(s.starts_with("\\version \"2.26.0\""));
        assert!(s.contains("\\language \"english\""));
        assert!(s.matches(" |").count() >= 100);
        for part in [
            "\\new Staff",
            "\\new TabStaff",
            "\\new NoteNames",
            "\\lyricsto",
            "\\layout",
            "\\midi",
            "treble_8",
        ] {
            assert!(s.contains(part), "missing {part}");
        }
        assert_eq!(generate_song(100), generate_song(100)); // deterministic
    }

    #[test]
    fn song_compiles_cleanly_when_lilypond_present() {
        if Command::new("lilypond").arg("--version").output().is_err() {
            println!("skipped: lilypond is not on PATH");
            return;
        }
        let dir = std::env::temp_dir().join(format!("swara-gen-song-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("song.ly");
        std::fs::write(&src, generate_song(100)).unwrap();
        let out = Command::new("lilypond")
            .env("LC_ALL", "C")
            .args(["-l", "WARNING", "-o"])
            .arg(dir.join("song"))
            .arg(&src)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(out.status.success(), "lilypond failed: {stderr}");
        assert_eq!(stderr, "", "lilypond printed warnings");
    }
}
