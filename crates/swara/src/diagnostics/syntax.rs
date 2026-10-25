//! Syntax errors from the tree: Tree-sitter's `ERROR` and `MISSING` nodes, in words.

use lsp_types::Diagnostic;
use tree_sitter::Node;

use super::error;
use crate::document::Document;
use crate::line_index::Encoding;

/// The most characters of a token an "unexpected" message quotes.
const QUOTED: usize = 20;

/// One diagnostic per `MISSING` node, "missing `<kind>`", and one per `ERROR` node, "unexpected
/// `<its first token>`" over that token, in the order they appear.
pub fn syntax_errors(doc: &Document, enc: Encoding) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    let mut stack = vec![doc.tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.is_missing() {
            let message = format!("missing `{}`", node.kind());
            found.push(error(
                doc,
                enc,
                node.start_byte(),
                node.start_byte(),
                message,
            ));
        } else if node.is_error() {
            let token = first_token(node);
            // A tree out of step with the text must cost a message, not the server.
            let text = doc.text.get(token.byte_range()).unwrap_or_default();
            let mut text = text.lines().next().unwrap_or("");
            if token == node {
                // An error node with no children is the unexpected text itself.
                text = text.split_whitespace().next().unwrap_or("");
            }
            let quoted: String = text.chars().take(QUOTED).collect();
            let start = token.start_byte();
            let message = format!("unexpected `{quoted}`");
            found.push(error(doc, enc, start, start + text.len(), message));
        } else {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).filter(Node::has_error).collect();
            stack.extend(children.into_iter().rev());
        }
    }
    found
}

fn first_token(mut node: Node) -> Node {
    while let Some(child) = node.child(0) {
        node = child;
    }
    node
}
