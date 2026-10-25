//! Diagnostics in three layers per file: `instant` from the tree on every edit, `check` from the
//! LilyPond run after a pause, and `build` from the save build. Every publish sends their union.

pub mod syntax;

use lsp_types::{Diagnostic, Position};
use tree_sitter::InputEdit;

use crate::document::Document;
use crate::line_index::Encoding;

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
        [&self.instant, &self.check, &self.build].into_iter().flatten().cloned().collect()
    }

    /// Moves the `check` and `build` entries after an edit. An entry inside the edited range is
    /// pinned to the range's start. `instant` is rebuilt from the tree after every edit instead.
    pub fn shift(&mut self, edit: &InputEdit) {
        for diagnostic in self.check.iter_mut().chain(&mut self.build) {
            diagnostic.range.start = shift_position(diagnostic.range.start, edit);
            diagnostic.range.end = shift_position(diagnostic.range.end, edit);
        }
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
pub fn in_encoding(mut diagnostics: Vec<Diagnostic>, doc: &Document, enc: Encoding) -> Vec<Diagnostic> {
    if enc == Encoding::Utf8 {
        return diagnostics;
    }
    let convert = |p: Position| {
        let byte = doc.index.position_to_byte(&doc.text, p, Encoding::Utf8);
        doc.index.byte_to_position(&doc.text, byte, enc)
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
            instant: vec![],
            check: vec![spanning(10, 2, 6), spanning(1, 2, 6)],
            build: vec![spanning(10, 0, 3)],
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
            instant: vec![],
            check: vec![spanning(4, 3, 5), spanning(4, 7, 8), spanning(4, 0, 4)],
            build: vec![],
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
}
