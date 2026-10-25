//! Converting between byte offsets in a document's text, LSP positions, and the line and column
//! LilyPond prints in its messages.

use lsp_types::Position;

/// How an LSP position counts characters within a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
}

/// The byte offset where each line starts. Only `\n` ends a line, as in Tree-sitter's rows, so a
/// `\r\n` line keeps its `\r` and positions past the end of a line stop before it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            text.bytes()
                .enumerate()
                .filter(|&(_, b)| b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self { starts }
    }

    /// A position past the end of its line means the end of the line, and one past the last line
    /// means the end of the text. A position inside a character means where it starts.
    pub fn position_to_byte(&self, text: &str, p: Position, enc: Encoding) -> usize {
        let Some((start, line)) = self.line(text, p.line) else {
            return text.len();
        };
        let character = p.character as usize;
        let offset = if line.is_ascii() {
            character.min(line.len())
        } else {
            match enc {
                Encoding::Utf8 => line.floor_char_boundary(character),
                Encoding::Utf16 => utf16_to_offset(line, character),
            }
        };
        start + offset
    }

    /// A byte inside a character means where it starts, and one past the end means the end.
    pub fn byte_to_position(&self, text: &str, byte: usize, enc: Encoding) -> Position {
        let byte = text.floor_char_boundary(byte);
        let line = self.starts.partition_point(|&start| start <= byte) - 1;
        let before = &text[self.starts[line]..byte];
        let character = match enc {
            Encoding::Utf16 if !before.is_ascii() => before.encode_utf16().count(),
            _ => before.len(),
        };
        Position::new(line as u32, character as u32)
    }

    /// LilyPond counts columns from 1, one per character, with a tab stop every 8 columns. A column
    /// past the end of the line means the end of the line.
    pub fn lilypond_column_to_byte(&self, text: &str, line0: u32, col1: u32) -> usize {
        let Some((start, line)) = self.line(text, line0) else {
            return text.len();
        };
        let mut column = 1;
        for (offset, ch) in line.char_indices() {
            let next = if ch == '\t' {
                (column - 1) / 8 * 8 + 9
            } else {
                column + 1
            };
            if col1 < next {
                return start + offset;
            }
            column = next;
        }
        start + line.len()
    }

    /// Where line `n` starts, and its text without the `\n` or `\r\n` that ends it.
    fn line<'t>(&self, text: &'t str, n: u32) -> Option<(usize, &'t str)> {
        let n = n as usize;
        let start = *self.starts.get(n)?;
        let end = self.starts.get(n + 1).map_or(text.len(), |next| next - 1);
        let line = &text[start..end];
        Some((start, line.strip_suffix('\r').unwrap_or(line)))
    }
}

/// The byte offset of the character that UTF-16 unit `units` falls in, or the end of the line.
fn utf16_to_offset(line: &str, units: usize) -> usize {
    let mut seen = 0;
    for (offset, ch) in line.char_indices() {
        seen += ch.len_utf16();
        if seen > units {
            return offset;
        }
    }
    line.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;

    const MIXED: &str = "a\tগান c4\n\"আমি\" d^\"𝄞\"";

    /// Every character boundary, plus the end of the text.
    fn boundaries(text: &str) -> Vec<usize> {
        text.char_indices()
            .map(|(i, _)| i)
            .chain([text.len()])
            .collect()
    }

    /// The position of `byte`, worked out from the text alone.
    fn expected(text: &str, byte: usize, enc: Encoding) -> Position {
        let before = &text[..byte];
        let line = before.matches('\n').count();
        let line_text = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
        let character = match enc {
            Encoding::Utf8 => line_text.len(),
            Encoding::Utf16 => line_text.encode_utf16().count(),
        };
        Position::new(line as u32, character as u32)
    }

    fn round_trip(enc: Encoding) {
        let index = LineIndex::new(MIXED);
        for byte in boundaries(MIXED) {
            let p = index.byte_to_position(MIXED, byte, enc);
            assert_eq!(p, expected(MIXED, byte, enc), "byte {byte}");
            assert_eq!(
                index.position_to_byte(MIXED, p, enc),
                byte,
                "position {p:?}"
            );
        }
    }

    #[test]
    fn utf8_round_trip() {
        round_trip(Encoding::Utf8);
        // A byte inside a character, and a UTF-8 position inside one, both mean where it starts.
        let index = LineIndex::new(MIXED);
        let clef = MIXED.find('𝄞').unwrap();
        let p = index.byte_to_position(MIXED, clef + 2, Encoding::Utf8);
        assert_eq!(index.position_to_byte(MIXED, p, Encoding::Utf8), clef);
    }

    #[test]
    fn utf16_round_trip() {
        round_trip(Encoding::Utf16);
        // 𝄞 is two UTF-16 units, and the position between them means where it starts.
        let index = LineIndex::new(MIXED);
        let clef = MIXED.find('𝄞').unwrap();
        let mut p = index.byte_to_position(MIXED, clef, Encoding::Utf16);
        p.character += 1;
        assert_eq!(index.position_to_byte(MIXED, p, Encoding::Utf16), clef);
    }

    #[test]
    fn positions_past_the_end_clamp() {
        let index = LineIndex::new(MIXED);
        let eol = MIXED.find('\n').unwrap();
        for enc in [Encoding::Utf8, Encoding::Utf16] {
            assert_eq!(
                index.position_to_byte(MIXED, Position::new(0, 99), enc),
                eol
            );
            assert_eq!(
                index.position_to_byte(MIXED, Position::new(9, 0), enc),
                MIXED.len()
            );
            assert_eq!(
                index.byte_to_position(MIXED, 999, enc),
                expected(MIXED, MIXED.len(), enc)
            );
        }
    }

    /// The byte LilyPond means by `column` on the second line of a file whose first line is
    /// `\version "2.26.0"`, as it reports them, and that byte's offset within the line.
    fn lilypond_byte(line: &str, column: u32) -> (usize, usize) {
        let text = format!("\\version \"2.26.0\"\n{line}\n");
        let start = text.find('\n').unwrap() + 1;
        let byte = LineIndex::new(&text).lilypond_column_to_byte(&text, 1, column);
        (byte, byte - start)
    }

    #[test]
    fn lilypond_non_bmp_column() {
        let line = "{ c4^\"𝄞\" } }";
        assert_eq!(lilypond_byte(line, 12).1, 14);
        let index = LineIndex::new(line);
        assert_eq!(
            index.byte_to_position(line, 14, Encoding::Utf16).character,
            12
        );
    }

    #[test]
    fn lilypond_tab_column() {
        assert_eq!(lilypond_byte("\tc4 \\foo", 12).1, 4);
    }

    #[test]
    fn lilypond_bengali_column() {
        assert_eq!(lilypond_byte("\"গান\" \\foo", 7).1, "\"গান\" ".len());
    }

    #[test]
    fn lilypond_column_past_the_end_clamps() {
        assert_eq!(lilypond_byte("{ c }", 40).1, 5);
    }

    /// The fixture's message names a line and column, and that column is the last `}` on it.
    fn fixture_points_at_last_brace(name: &str) {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/lilypond/"
        );
        let text = std::fs::read_to_string(format!("{dir}{name}.ly")).unwrap();
        let stderr = std::fs::read_to_string(format!("{dir}{name}.stderr")).unwrap();
        let mut fields = stderr.lines().next().unwrap().split(':').skip(1);
        let line: u32 = fields.next().unwrap().parse().unwrap();
        let column: u32 = fields.next().unwrap().parse().unwrap();
        let line_text = text.lines().nth(line as usize - 1).unwrap();
        let start = text.find(line_text).unwrap();
        let index = LineIndex::new(&text);
        let byte = index.lilypond_column_to_byte(&text, line - 1, column);
        assert_eq!(byte, start + line_text.rfind('}').unwrap(), "{name}");
    }

    #[test]
    fn lilypond_fixture_columns() {
        fixture_points_at_last_brace("tab-column");
        fixture_points_at_last_brace("utf8-column");
    }

    #[test]
    fn crlf_lines() {
        let lf = "a\nbc\n\nd";
        let crlf = lf.replace('\n', "\r\n");
        let (lf_index, crlf_index) = (LineIndex::new(lf), LineIndex::new(&crlf));
        for line in 0..4u32 {
            let start = |index: &LineIndex, text: &str| {
                index.position_to_byte(text, Position::new(line, 0), Encoding::Utf8)
            };
            assert_eq!(
                start(&crlf_index, &crlf),
                start(&lf_index, lf) + line as usize
            );
        }
        // The end of a line is before its `\r`.
        assert_eq!(
            crlf_index.position_to_byte(&crlf, Position::new(1, 9), Encoding::Utf16),
            5
        );
        assert_eq!(crlf_index.lilypond_column_to_byte(&crlf, 1, 9), 5);
    }
}
