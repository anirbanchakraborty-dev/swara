//! The outline of a file: its variables, and its scores, books and book parts.

use lsp_types::{DocumentSymbol, Range, SymbolKind};
use tree_sitter::Node;

use crate::document::Document;
use crate::line_index::Encoding;
use crate::node_text;

/// The variables a file assigns at its top level, and its `\score`, `\book` and `\bookpart` blocks
/// with the blocks each holds.
pub fn document_symbols(doc: &Document, enc: Encoding) -> Vec<DocumentSymbol> {
    let outline = Outline { doc, enc };
    let root = doc.tree.root_node();
    let mut symbols = Vec::new();
    let mut cursor = root.walk();
    for node in root.named_children(&mut cursor) {
        if node.kind() == "assignment" {
            symbols.extend(outline.variable(node));
        } else {
            outline.blocks(node, &mut symbols);
        }
    }
    symbols
}

struct Outline<'a> {
    doc: &'a Document,
    enc: Encoding,
}

impl Outline<'_> {
    fn variable(&self, node: Node) -> Option<DocumentSymbol> {
        let name = node.child_by_field_name("name")?;
        let text = &self.doc.text;
        let label = match name.kind() {
            "string" => node_text::string(name, text),
            _ => node_text::text(name, text).to_owned(),
        };
        Some(self.symbol(
            label,
            SymbolKind::VARIABLE,
            node,
            self.range(name.start_byte(), name.end_byte()),
            None,
        ))
    }

    /// The blocks in `node`, or `node` itself when it is one.
    fn blocks(&self, node: Node, out: &mut Vec<DocumentSymbol>) {
        let keyword = match node.kind() {
            "score_block" => "\\score",
            "book_block" => "\\book",
            "bookpart_block" => "\\bookpart",
            _ => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.blocks(child, out);
                }
                return;
            }
        };
        let mut children = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.blocks(child, &mut children);
        }
        let name = match self.title(node) {
            Some(title) => format!("{keyword}: {title}"),
            None => keyword.to_owned(),
        };
        let start = node.start_byte();
        let selection = self.range(start, (start + keyword.len()).min(node.end_byte()));
        out.push(self.symbol(name, SymbolKind::MODULE, node, selection, Some(children)));
    }

    /// The `title`, or failing that the `piece`, of the block's own `\header`.
    fn title(&self, block: Node) -> Option<String> {
        let text = &self.doc.text;
        let mut cursor = block.walk();
        let header = block
            .named_children(&mut cursor)
            .find(|n| n.kind() == "header_block")?;
        let mut fields = Vec::new();
        let mut cursor = header.walk();
        for field in header
            .named_children(&mut cursor)
            .filter(|n| n.kind() == "assignment")
        {
            let name = field.child_by_field_name("name");
            let value = field
                .child_by_field_name("value")
                .filter(|v| v.kind() == "string");
            if let (Some(name), Some(value)) = (name, value) {
                fields.push((node_text::text(name, text), node_text::string(value, text)));
            }
        }
        ["title", "piece"]
            .into_iter()
            .find_map(|wanted| fields.iter().find(|(name, _)| *name == wanted))
            .map(|(_, value)| value.clone())
    }

    #[allow(deprecated)]
    fn symbol(
        &self,
        name: String,
        kind: SymbolKind,
        node: Node,
        selection_range: Range,
        children: Option<Vec<DocumentSymbol>>,
    ) -> DocumentSymbol {
        DocumentSymbol {
            name,
            detail: None,
            kind,
            tags: None,
            deprecated: None,
            range: self.range(node.start_byte(), node.end_byte()),
            selection_range,
            children: children.filter(|c| !c.is_empty()),
        }
    }

    fn range(&self, start: usize, end: usize) -> Range {
        let at = |byte| {
            self.doc
                .index
                .byte_to_position(&self.doc.text, byte, self.enc)
        };
        Range::new(at(start), at(end))
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Range, SymbolKind};

    use super::*;
    use crate::testing::document;

    /// Each symbol's name and kind, with its children's after it, indented by depth.
    fn outline(symbols: &[DocumentSymbol], depth: usize, out: &mut Vec<String>) {
        for symbol in symbols {
            out.push(format!(
                "{}{} {:?}",
                "  ".repeat(depth),
                symbol.name,
                symbol.kind
            ));
            outline(
                symbol.children.as_deref().unwrap_or_default(),
                depth + 1,
                out,
            );
        }
    }

    #[test]
    fn outline_lists_variables_and_scores() {
        let source = "melody = { c }\n\
                      \"my tune\" = { d }\n\
                      \\header { title = \"Song\" }\n\
                      \\score { \\header { piece = \"Allegro\" } { \\melody } }\n\
                      \\book { \\header { title = \"Book\" } \\bookpart { \\score { c } } }\n";
        let symbols = document_symbols(&document(None, source), Encoding::Utf8);
        let mut lines = Vec::new();
        outline(&symbols, 0, &mut lines);
        assert_eq!(
            lines,
            [
                "melody Variable",
                "my tune Variable",
                "\\score: Allegro Module",
                "\\book: Book Module",
                "  \\bookpart Module",
                "    \\score Module",
            ]
        );
        let melody = &symbols[0];
        assert_eq!(melody.kind, SymbolKind::VARIABLE);
        let at = |line, character| Position::new(line, character);
        assert_eq!(melody.range, Range::new(at(0, 0), at(0, 14)));
        assert_eq!(melody.selection_range, Range::new(at(0, 0), at(0, 6)));
        let score = &symbols[2];
        assert_eq!(score.range, Range::new(at(3, 0), at(3, 52)));
        assert_eq!(score.selection_range, Range::new(at(3, 0), at(3, 6)));
    }

    #[test]
    fn outline_positions_follow_the_encoding() {
        // `স` is three bytes in UTF-8 and one unit in UTF-16.
        let symbols = document_symbols(&document(None, "\"সুর\" = { c }\n"), Encoding::Utf16);
        assert_eq!(symbols[0].name, "সুর");
        assert_eq!(symbols[0].selection_range.end, Position::new(0, 5));
    }
}
