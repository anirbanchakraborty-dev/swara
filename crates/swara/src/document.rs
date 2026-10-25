//! An open file: its text, the tree parsed from it, and where each line starts.

use lsp_types::{TextDocumentContentChangeEvent, Uri};
use tree_sitter::{InputEdit, Parser, Point, Tree};

use crate::line_index::{Encoding, LineIndex};

pub struct Document {
    pub uri: Uri,
    pub version: i32,
    pub text: String,
    pub tree: Tree,
    pub index: LineIndex,
}

impl Document {
    pub fn new(uri: Uri, version: i32, text: String, parser: &mut Parser) -> Self {
        let tree = parse(parser, &text, None);
        let index = LineIndex::new(&text);
        Self {
            uri,
            version,
            text,
            tree,
            index,
        }
    }

    /// Applies one change and parses again. A change with a range edits the old tree first, so the
    /// parse reuses everything the change left alone, and the edit is returned. A change without a
    /// range replaces the whole text, and there is no edit to return.
    pub fn apply(
        &mut self,
        change: &TextDocumentContentChangeEvent,
        enc: Encoding,
        parser: &mut Parser,
    ) -> Option<InputEdit> {
        let Some(range) = change.range else {
            self.text.clone_from(&change.text);
            self.index = LineIndex::new(&self.text);
            self.tree = parse(parser, &self.text, None);
            return None;
        };
        let start_byte = self.index.position_to_byte(&self.text, range.start, enc);
        let old_end_byte = self
            .index
            .position_to_byte(&self.text, range.end, enc)
            .max(start_byte);
        let start_position = self.point(start_byte);
        let old_end_position = self.point(old_end_byte);
        self.text
            .replace_range(start_byte..old_end_byte, &change.text);
        self.index = LineIndex::new(&self.text);
        let new_end_byte = start_byte + change.text.len();
        let edit = InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position,
            old_end_position,
            new_end_position: self.point(new_end_byte),
        };
        self.tree.edit(&edit);
        self.tree = parse(parser, &self.text, Some(&self.tree));
        Some(edit)
    }

    /// Tree-sitter's point for a byte: its line and its byte column.
    fn point(&self, byte: usize) -> Point {
        let p = self
            .index
            .byte_to_position(&self.text, byte, Encoding::Utf8);
        Point::new(p.line as usize, p.character as usize)
    }
}

fn parse(parser: &mut Parser, text: &str, old: Option<&Tree>) -> Tree {
    parser
        .parse(text, old)
        .expect("a parser with a language and no cancellation returns a tree")
}
