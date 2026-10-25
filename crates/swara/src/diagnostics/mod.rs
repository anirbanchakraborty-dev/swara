//! Diagnostics in three layers per file: `instant` from the tree on every edit, `check` from the
//! LilyPond run after a pause, and `build` from the save build. Every publish sends their union.

pub mod includes;
pub mod names;
pub mod syntax;

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use tree_sitter::InputEdit;

use crate::document::Document;
use crate::line_index::{Encoding, LineIndex};

/// Positions here are UTF-8, a line and a byte column like Tree-sitter's points, so an edit moves
/// them exactly. They are converted to the client's encoding when published.
#[derive(Debug, Default)]
pub struct Layers {
    pub instant: Vec<Diagnostic>,
    pub check: Vec<Diagnostic>,
    pub build: Vec<Diagnostic>,
}

impl Layers {
    pub fn merged(&self) -> Vec<Diagnostic> {
        [&self.instant, &self.check, &self.build]
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }

    /// Moves the `check` and `build` entries after an edit. An entry inside the edited range is
    /// pinned to the range's start. `instant` is rebuilt from the tree after every edit instead.
    pub fn shift(&mut self, edit: &InputEdit) {
        shift_all(&mut self.check, edit);
        shift_all(&mut self.build, edit);
    }

    pub fn set_check(&mut self, diagnostics: Vec<Diagnostic>) {
        self.check = diagnostics;
        self.drop_repeats();
    }

    pub fn set_build(&mut self, diagnostics: Vec<Diagnostic>) {
        self.build = diagnostics;
        self.drop_repeats();
    }

    /// A check and a build both report the parse errors, and the editor would show each twice.
    /// The build keeps only what the check does not hold, which leaves the engraving warnings.
    /// Both layers move through the same edits, so a message both found stays equal in both.
    fn drop_repeats(&mut self) {
        let check = &self.check;
        self.build.retain(|d| !check.contains(d));
    }
}

/// Moves each of `diagnostics` after an edit, as `Layers::shift` does.
pub fn shift_all(diagnostics: &mut [Diagnostic], edit: &InputEdit) {
    for diagnostic in diagnostics {
        diagnostic.range.start = shift_position(diagnostic.range.start, edit);
        diagnostic.range.end = shift_position(diagnostic.range.end, edit);
    }
}

/// An error over the bytes `start..end` of `doc`, with positions in `enc`.
pub(crate) fn error(
    doc: &Document,
    enc: Encoding,
    start: usize,
    end: usize,
    message: String,
) -> Diagnostic {
    let position = |byte| doc.index.byte_to_position(&doc.text, byte, enc);
    Diagnostic {
        range: Range::new(position(start), position(end)),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("swara".into()),
        message,
        ..Default::default()
    }
}

fn shift_position(p: Position, edit: &InputEdit) -> Position {
    let here = (p.line as usize, p.character as usize);
    let start = (edit.start_position.row, edit.start_position.column);
    let old_end = (edit.old_end_position.row, edit.old_end_position.column);
    let new_end = (edit.new_end_position.row, edit.new_end_position.column);
    let (line, character) = if here < start {
        here
    } else if here < old_end {
        start
    } else if here.0 == old_end.0 {
        (new_end.0, new_end.1 + here.1 - old_end.1)
    } else {
        (here.0 - old_end.0 + new_end.0, here.1)
    };
    Position::new(line as u32, character as u32)
}

/// Converts layer positions to the client's encoding.
pub fn in_encoding(diagnostics: Vec<Diagnostic>, doc: &Document, enc: Encoding) -> Vec<Diagnostic> {
    convert(diagnostics, &doc.text, &doc.index, enc)
}

/// Converts UTF-8 positions in `text` to the client's encoding.
pub fn convert(
    mut diagnostics: Vec<Diagnostic>,
    text: &str,
    index: &LineIndex,
    enc: Encoding,
) -> Vec<Diagnostic> {
    if enc == Encoding::Utf8 {
        return diagnostics;
    }
    let convert = |p: Position| {
        let byte = index.position_to_byte(text, p, Encoding::Utf8);
        index.byte_to_position(text, byte, enc)
    };
    for diagnostic in &mut diagnostics {
        diagnostic.range.start = convert(diagnostic.range.start);
        diagnostic.range.end = convert(diagnostic.range.end);
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Diagnostic, Position, Range};
    use tree_sitter::{InputEdit, Point};

    fn spanning(line: u32, from: u32, to: u32) -> Diagnostic {
        Diagnostic {
            range: Range::new(Position::new(line, from), Position::new(line, to)),
            message: format!("{line}:{from}"),
            ..Default::default()
        }
    }

    fn edit(start: (usize, usize), old_end: (usize, usize), new_end: (usize, usize)) -> InputEdit {
        // The byte offsets do not matter to `shift`, which works on lines and columns.
        InputEdit {
            start_byte: 0,
            old_end_byte: 0,
            new_end_byte: 0,
            start_position: Point::new(start.0, start.1),
            old_end_position: Point::new(old_end.0, old_end.1),
            new_end_position: Point::new(new_end.0, new_end.1),
        }
    }

    #[test]
    fn layers_shift() {
        let mut layers = Layers {
            check: vec![spanning(10, 2, 6), spanning(1, 2, 6)],
            build: vec![spanning(10, 0, 3)],
            ..Layers::default()
        };
        // Two lines inserted at the start of line 3.
        layers.shift(&edit((3, 0), (3, 0), (5, 0)));
        assert_eq!(layers.check[0].range, spanning(12, 2, 6).range);
        assert_eq!(layers.check[1].range, spanning(1, 2, 6).range);
        assert_eq!(layers.build[0].range, spanning(12, 0, 3).range);
    }

    #[test]
    fn shift_on_the_edited_line() {
        let mut layers = Layers {
            check: vec![spanning(4, 3, 5), spanning(4, 7, 8), spanning(4, 0, 4)],
            ..Layers::default()
        };
        // Columns 2 to 6 of line 4 replaced by two bytes.
        layers.shift(&edit((4, 2), (4, 6), (4, 4)));
        // Inside the edit: pinned to its start. After it: moved left by two.
        assert_eq!(layers.check[0].range, spanning(4, 2, 2).range);
        assert_eq!(layers.check[1].range, spanning(4, 5, 6).range);
        assert_eq!(layers.check[2].range, spanning(4, 0, 2).range);
    }

    #[test]
    fn merged_is_the_union() {
        let layers = Layers {
            instant: vec![spanning(0, 0, 1)],
            check: vec![spanning(1, 0, 1)],
            build: vec![spanning(2, 0, 1)],
        };
        let lines: Vec<u32> = layers.merged().iter().map(|d| d.range.start.line).collect();
        assert_eq!(lines, [0, 1, 2]);
    }

    fn lines(diagnostics: &[Diagnostic]) -> Vec<u32> {
        diagnostics.iter().map(|d| d.range.start.line).collect()
    }

    #[test]
    fn build_keeps_only_what_the_check_missed() {
        let mut layers = Layers::default();
        layers.set_check(vec![spanning(1, 0, 1)]);
        layers.set_build(vec![spanning(1, 0, 1), spanning(5, 0, 1)]);
        assert_eq!(lines(&layers.build), [5]);
        assert_eq!(lines(&layers.merged()), [1, 5]);
    }

    #[test]
    fn check_arriving_after_the_build_trims_it() {
        let mut layers = Layers::default();
        layers.set_build(vec![spanning(1, 0, 1), spanning(5, 0, 1)]);
        layers.set_check(vec![spanning(1, 0, 1)]);
        assert_eq!(lines(&layers.build), [5]);
    }

    #[test]
    fn build_only_messages_outlive_later_checks() {
        let mut layers = Layers::default();
        layers.set_build(vec![spanning(5, 0, 1)]);
        layers.set_check(vec![spanning(1, 0, 1)]);
        layers.shift(&edit((0, 0), (0, 0), (2, 0)));
        layers.set_check(vec![]);
        assert_eq!(lines(&layers.build), [7]);
    }
}
