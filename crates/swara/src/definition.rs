//! Where a variable the cursor is on was assigned.

use lsp_types::Location;
use tree_sitter::Node;

use crate::document::Document;
use crate::file_uri;
use crate::index::{Index, visible_definitions};
use crate::node_text;

/// Where the `\name` at byte `pos` of `doc` is assigned, in this file or one it can see. The range
/// is in UTF-8 positions, as the index keeps them.
pub fn definition(doc: &Document, pos: usize, index: &Index) -> Option<Location> {
    let command = command_at(doc, pos)?;
    let name = node_text::command(command, &doc.text);
    let own = file_uri::to_path(&doc.uri);
    let found = visible_definitions(doc, index)
        .into_iter()
        .find(|d| d.name == name)?;
    // An untitled file's own definitions have an empty path.
    let here = match &own {
        Some(own) => *own == found.path,
        None => found.path.as_os_str().is_empty(),
    };
    let uri = if here {
        doc.uri.clone()
    } else {
        file_uri::from_path(&found.path)?
    };
    Some(Location::new(uri, found.range))
}

/// The `command` the cursor at byte `pos` is in, or just after.
fn command_at(doc: &Document, pos: usize) -> Option<Node<'_>> {
    let root = doc.tree.root_node();
    [Some(pos), pos.checked_sub(1)]
        .into_iter()
        .flatten()
        .filter_map(|at| root.named_descendant_for_byte_range(at, at))
        .find(|n| n.kind() == "command" && n.start_byte() <= pos && pos <= n.end_byte())
}

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Range};

    use super::*;
    use crate::file_uri;
    use crate::testing::{Scratch, document};

    /// Where the cursor is in a test's source. `|` would be a bar check.
    const CURSOR: char = '‸';

    fn split(source: &str) -> (String, usize) {
        let pos = source.find(CURSOR).expect("a cursor in the source");
        (source.replacen(CURSOR, "", 1), pos)
    }

    fn range(start: (u32, u32), end: (u32, u32)) -> Range {
        Range::new(Position::new(start.0, start.1), Position::new(end.0, end.1))
    }

    /// Where `definition` goes from the cursor in an untitled document holding `source`.
    fn found(source: &str) -> Option<Location> {
        let (text, pos) = split(source);
        let doc = document(None, &text);
        definition(&doc, pos, &Index::new(Vec::new()))
    }

    #[test]
    fn goes_to_same_file_assignment() {
        let location = found("melody = { c }\n{ \\mel‸ody }").expect("a definition");
        assert_eq!(location.uri.as_str(), "untitled:Untitled-1");
        assert_eq!(location.range, range((0, 0), (0, 6)));
        // At either end of the name too.
        assert!(found("melody = { c }\n{ ‸\\melody }").is_some());
        assert!(found("melody = { c }\n{ \\melody‸ }").is_some());
    }

    #[test]
    fn goes_to_included_assignment() {
        let dir = Scratch::new("definition-includes");
        let defs = dir.write("defs.ily", "% the tune\ntune = { c }\n");
        let path = dir.path().join("main.ly");
        let (text, pos) = split("\\include \"defs.ily\"\n{ \\tu‸ne }\n");
        let doc = document(Some(&path), &text);
        let mut index = Index::new(Vec::new());
        index.update(&path, &doc);
        let location = definition(&doc, pos, &index).expect("a definition");
        assert_eq!(Some(location.uri), file_uri::from_path(&defs));
        assert_eq!(location.range, range((1, 0), (1, 4)));
    }

    #[test]
    fn string_named_variable() {
        let location = found("\"my tune\" = { c }\n{ \\\"my t‸une\" }").expect("a definition");
        assert_eq!(location.range, range((0, 0), (0, 9)));
    }

    #[test]
    fn nothing_for_lilypond_names_or_notes() {
        assert_eq!(found("{ \\rel‸ative c' { c } }"), None);
        assert_eq!(found("melody = { c }\n{ mel‸ody }"), None);
        assert_eq!(found("melody = { c }\n{ c‸ }"), None);
    }
}
