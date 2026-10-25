//! Includes that name no file.

use lsp_types::Diagnostic;
use tree_sitter::Node;

use super::error;
use crate::commanddata::CommandData;
use crate::document::Document;
use crate::file_uri;
use crate::index::Index;
use crate::line_index::Encoding;
use crate::node_text;

/// Every `\include` in `doc` that resolves to no file. Nothing is reported until LilyPond's
/// command data has loaded, since its data folder holds the files LilyPond ships, or for a file
/// that is not saved, since there is no folder to search from.
pub fn include_checks(
    doc: &Document,
    data: Option<&CommandData>,
    index: &Index,
    enc: Encoding,
) -> Vec<Diagnostic> {
    let Some(path) = file_uri::to_path(&doc.uri) else {
        return Vec::new();
    };
    if data.is_none() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut check = |file: Node| {
        let name = node_text::string(file, &doc.text);
        if index.resolve_include(&path, &name).is_none() {
            found.push(error(
                doc,
                enc,
                file.start_byte(),
                file.end_byte(),
                format!("cannot find `{name}`"),
            ));
        }
    };
    // Each node is visited before what it holds, and siblings in the order they are written.
    let mut stack = vec![doc.tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "include" {
            if let Some(file) = node.child_by_field_name("path") {
                check(file);
            }
        } else if node.kind() == "command"
            && node_text::command(node, &doc.text) == "include"
            && let Some(file) = next_argument(node)
            && file.kind() == "string"
        {
            check(file);
        }
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    found
}

/// The node after `node` in its parent, past any comments.
fn next_argument(node: Node) -> Option<Node> {
    std::iter::successors(node.next_named_sibling(), |n| n.next_named_sibling())
        .find(|n| !n.is_extra())
}

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Range};

    use super::*;
    use crate::testing::{Scratch, document, mini};

    fn checked(doc: &Document, index: &Index) -> Vec<Diagnostic> {
        let data = mini();
        include_checks(doc, Some(&data), index, Encoding::Utf8)
    }

    fn messages(diagnostics: &[Diagnostic]) -> Vec<&str> {
        diagnostics.iter().map(|d| d.message.as_str()).collect()
    }

    #[test]
    fn missing_include_flagged() {
        let dir = Scratch::new("includes-missing");
        let main = dir.path().join("main.ly");
        let doc = document(
            Some(&main),
            "\\include \"nothere.ily\"\n{ \\include \"gone.ily\" }\n",
        );
        let found = checked(&doc, &Index::new(Vec::new()));
        assert_eq!(
            messages(&found),
            ["cannot find `nothere.ily`", "cannot find `gone.ily`"]
        );
        assert_eq!(
            found[0].range,
            Range::new(Position::new(0, 9), Position::new(0, 22))
        );
    }

    #[test]
    fn datadir_include_found() {
        let dir = Scratch::new("includes-datadir");
        dir.write("share/ly/english.ly", "\\language \"english\"\n");
        let main = dir.path().join("main.ly");
        let doc = document(Some(&main), "\\include \"english.ly\"\n");
        let mut index = Index::new(Vec::new());
        assert_eq!(
            messages(&checked(&doc, &index)),
            ["cannot find `english.ly`"]
        );
        index.set_datadir(Some(dir.path().join("share")));
        assert!(checked(&doc, &index).is_empty());
    }

    #[test]
    fn bengali_folder_include() {
        let dir = Scratch::new("includes-bengali");
        dir.write("আমার গান/sargam.ily", "sa = { c }\n");
        let index = Index::new(Vec::new());
        let inside = dir.path().join("আমার গান/main.ly");
        let doc = document(Some(&inside), "\\include \"sargam.ily\"\n");
        assert!(checked(&doc, &index).is_empty());
        let above = dir.path().join("main.ly");
        let doc = document(Some(&above), "\\include \"আমার গান/sargam.ily\"\n");
        assert!(checked(&doc, &index).is_empty());
    }

    #[test]
    fn includes_wait_for_data() {
        let dir = Scratch::new("includes-wait");
        let main = dir.path().join("main.ly");
        let doc = document(Some(&main), "\\include \"nothere.ily\"\n");
        assert!(include_checks(&doc, None, &Index::new(Vec::new()), Encoding::Utf8).is_empty());
    }
}
