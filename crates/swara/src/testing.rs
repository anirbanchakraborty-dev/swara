//! What the unit tests share: LilyPond's command data from the hand-written fixture, documents
//! parsed from text, and folders that remove themselves.

use std::fs;
use std::path::{Path, PathBuf};

use tree_sitter::{Node, Parser};

use crate::commanddata::{self, CommandData};
use crate::document::Document;
use crate::file_uri;

const MINI: &str = include_str!("../tests/fixtures/commanddata/mini.jsonl");

pub fn mini() -> CommandData {
    commanddata::parse(MINI).expect("the fixture parses")
}

/// A document holding `source`, at `path` when there is one, and untitled otherwise.
pub fn document(path: Option<&Path>, source: &str) -> Document {
    let uri = match path {
        Some(path) => file_uri::from_path(path).expect("a test path makes a URI"),
        None => "untitled:Untitled-1".parse().expect("a valid URI"),
    };
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_lilypond::LANGUAGE.into())
        .expect("the grammar loads");
    Document::new(uri, 1, source.to_owned(), &mut parser)
}

/// The first node of `kind` at or under `node`, in source order.
pub fn first<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    let children: Vec<Node<'t>> = node.children(&mut cursor).collect();
    children.into_iter().find_map(|child| first(child, kind))
}

/// A fresh folder for one test, removed when the test ends, whether it passes or not.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("swara-unit-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `text` to `name` inside the folder, making the folders it names, and returns its
    /// path.
    pub fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
