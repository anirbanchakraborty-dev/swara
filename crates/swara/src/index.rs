//! What each file defines and includes, across the open files and the files they include from
//! disk, so a name defined in one file is known in another.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use lsp_types::{Position, Range};
use tree_sitter::{Node, Parser, Tree};

use crate::commanddata::Arg;
use crate::document::Document;
use crate::file_uri;
use crate::node_text;

/// The note-name language LilyPond starts in.
pub const DEFAULT_LANGUAGE: &str = "nederlands";

/// The note-name languages that may be active at a point, one for each way the file can be
/// reached, or `None` when it cannot be known, such as in an `.ily` file nothing includes.
pub type Languages = Option<BTreeSet<String>>;

// Going to a definition reads `path` and `range`, in a later task.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Definition {
    pub name: String,
    pub path: PathBuf,
    /// Where the name is written, in UTF-8 positions.
    pub range: Range,
    /// The arguments, when the value is a music, scheme, void or event function.
    pub signature: Option<Vec<Arg>>,
}

/// What a file's top level defines, and the includes and language changes in order.
#[derive(Debug, Default)]
pub struct Summary {
    pub definitions: Vec<Definition>,
    steps: Vec<Step>,
}

#[derive(Debug, PartialEq)]
enum Step {
    Include(String),
    Language(String),
}

impl Summary {
    /// Whether `other` defines the same names with the same signatures, and includes and changes
    /// language in the same order. Where each name is written does not count.
    fn same_outline(&self, other: &Summary) -> bool {
        self.steps == other.steps
            && self.definitions.len() == other.definitions.len()
            && self
                .definitions
                .iter()
                .zip(&other.definitions)
                .all(|(a, b)| a.name == b.name && a.signature == b.signature)
    }
}

/// What `doc` can name: its own definitions, then those of the files it reaches through includes
/// and of the open files that include it. A name defined in two files appears twice.
pub fn visible_definitions(doc: &Document, index: &Index) -> Vec<Definition> {
    let path = file_uri::to_path(&doc.uri);
    let own = summarize(&doc.tree, &doc.text, path.as_deref().unwrap_or(Path::new("")));
    let mut definitions = own.definitions;
    if let Some(path) = &path {
        // The index holds this file too, perhaps from an older version of it.
        let others = index.definitions_visible_from(path).into_iter();
        definitions.extend(others.filter(|d| d.path != *path));
    }
    definitions
}

/// The Scheme forms that make a function a music function can call with arguments.
const FUNCTION_MAKERS: &[&str] = &[
    "define-music-function",
    "define-scheme-function",
    "define-void-function",
    "define-event-function",
];

/// The definitions in a file's top level: assignments, and Scheme `define` forms.
pub fn summarize(tree: &Tree, text: &str, path: &Path) -> Summary {
    let mut summary = Summary::default();
    let root = tree.root_node();
    let mut cursor = root.walk();
    for node in root.named_children(&mut cursor) {
        match node.kind() {
            "assignment" => {
                let Some(name) = node.child_by_field_name("name") else {
                    continue;
                };
                let name = match name.kind() {
                    "string" => Some((node_text::string(name, text), name)),
                    "identifier" => Some((node_text::text(name, text).to_owned(), name)),
                    "dotted_name" => name
                        .named_child(0)
                        .map(|first| (node_text::text(first, text).to_owned(), first)),
                    _ => None,
                };
                let Some((name, at)) = name else { continue };
                let signature = node
                    .child_by_field_name("value")
                    .and_then(|v| signature(v, text));
                summary
                    .definitions
                    .push(definition(name, at, path, signature));
            }
            "include" => {
                if let Some(file) = node.child_by_field_name("path") {
                    summary
                        .steps
                        .push(Step::Include(node_text::string(file, text)));
                }
            }
            "language" => {
                if let Some(name) = node.child_by_field_name("name") {
                    summary
                        .steps
                        .push(Step::Language(node_text::string(name, text)));
                }
            }
            "scheme" => {
                if let Some(found) = scheme_definition(node, text, path) {
                    summary.definitions.push(found);
                }
            }
            _ => {}
        }
    }
    summary
}

fn definition(name: String, at: Node, path: &Path, signature: Option<Vec<Arg>>) -> Definition {
    let point = |p: tree_sitter::Point| Position::new(p.row as u32, p.column as u32);
    Definition {
        name,
        path: path.to_owned(),
        range: Range::new(point(at.start_position()), point(at.end_position())),
        signature,
    }
}

/// The elements of a Scheme list, without its comments.
fn elements<'t>(list: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .filter(|n| !n.is_extra() && !n.kind().ends_with("comment"))
        .collect()
}

/// The list a `#` or `$` expression holds, or the node itself when it is a list.
fn as_list(node: Node) -> Option<Node> {
    let list = if node.kind() == "scheme" {
        node.named_child(0)?
    } else {
        node
    };
    (list.kind() == "scheme_list").then_some(list)
}

fn symbol<'s>(node: Node, text: &'s str) -> Option<&'s str> {
    (node.kind() == "scheme_symbol").then(|| node_text::text(node, text))
}

/// The name a top-level `#(define ...)`, `#(define-markup-command ...)` or similar form
/// defines.
fn scheme_definition(node: Node, text: &str, path: &Path) -> Option<Definition> {
    let items = elements(as_list(node)?);
    let head = symbol(*items.first()?, text)?;
    let target = *items.get(1)?;
    match head {
        "define" | "define*" | "define-public" | "define*-public" => {
            // `(define name value)`, or `(define (name args) body)` for a procedure.
            let at = if target.kind() == "scheme_list" {
                *elements(target).first()?
            } else {
                target
            };
            let name = symbol(at, text)?.to_owned();
            let signature = items.get(2).and_then(|v| signature(*v, text));
            Some(definition(name, at, path, signature))
        }
        "define-markup-command" | "define-markup-list-command" => {
            let at = *elements(target).first()?;
            Some(definition(symbol(at, text)?.to_owned(), at, path, None))
        }
        _ => None,
    }
}

/// The arguments of a function made with `define-music-function` and its kin. A predicate
/// written alone is required, and one written as `(pred? default)` is optional.
fn signature(value: Node, text: &str) -> Option<Vec<Arg>> {
    let items = elements(as_list(value)?);
    if !FUNCTION_MAKERS.contains(&symbol(*items.first()?, text)?) {
        return None;
    }
    let preds = as_list(*items.get(2)?)?;
    let args = elements(preds)
        .into_iter()
        .filter_map(|p| match p.kind() {
            "scheme_symbol" => Some(Arg {
                pred: node_text::text(p, text).to_owned(),
                optional: false,
            }),
            "scheme_list" => {
                let pred = symbol(*elements(p).first()?, text)?;
                Some(Arg {
                    pred: pred.to_owned(),
                    optional: true,
                })
            }
            _ => None,
        })
        .collect();
    Some(args)
}

/// Files read from disk, each with the time it was last changed when it was read.
type DiskCache = HashMap<PathBuf, (Option<SystemTime>, Rc<Summary>)>;

pub struct Index {
    include_paths: Vec<String>,
    datadir: Option<PathBuf>,
    open: HashMap<PathBuf, Rc<Summary>>,
    disk: RefCell<DiskCache>,
    parser: RefCell<Parser>,
}

impl Index {
    /// `include_paths` are searched after the including file's own folder. A relative one is
    /// taken from that folder.
    pub fn new(include_paths: Vec<String>) -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_lilypond::LANGUAGE.into())
            .expect("the grammar loads");
        Self {
            include_paths,
            datadir: None,
            open: HashMap::new(),
            disk: RefCell::new(HashMap::new()),
            parser: RefCell::new(parser),
        }
    }

    /// LilyPond's data folder, whose `ly/` holds the files it ships, such as `english.ly`.
    pub fn set_datadir(&mut self, datadir: Option<PathBuf>) {
        self.datadir = datadir;
    }

    /// Records an open file. Returns whether its outline changed: the names it defines, their
    /// signatures, or its includes and language changes.
    pub fn update(&mut self, path: &Path, doc: &Document) -> bool {
        let summary = summarize(&doc.tree, &doc.text, path);
        let key = canonical(path);
        let changed = self
            .open
            .get(&key)
            .is_none_or(|old| !old.same_outline(&summary));
        self.open.insert(key, Rc::new(summary));
        changed
    }

    /// Forgets an open file, so it is read from disk again.
    pub fn close(&mut self, path: &Path) {
        self.open.remove(&canonical(path));
    }

    /// The file `\include "text"` in `from` names: in `from`'s folder, then in each include path,
    /// then in LilyPond's `ly/` folder.
    pub fn resolve_include(&self, from: &Path, text: &str) -> Option<PathBuf> {
        let dir = from.parent()?;
        let searched = self.include_paths.iter().map(|p| dir.join(p));
        let shipped = self.datadir.iter().map(|d| d.join("ly"));
        std::iter::once(dir.to_owned())
            .chain(searched)
            .chain(shipped)
            .map(|folder| normal(&folder.join(text)))
            .find(|candidate| self.open.contains_key(&canonical(candidate)) || candidate.is_file())
    }

    /// Every definition `root` can see: its own, those of the files it includes, and those of the
    /// open files that include it, with everything they include.
    pub fn definitions_visible_from(&self, root: &Path) -> Vec<Definition> {
        let key = canonical(root);
        let mut seen = HashSet::new();
        let mut found = Vec::new();
        self.collect(root, &mut seen, &mut found);
        for path in self.open.keys() {
            if !seen.contains(path) && self.closure(path).iter().any(|(p, _)| *p == key) {
                self.collect(path, &mut seen, &mut found);
            }
        }
        found
    }

    /// The languages active where `path` starts, from the files that include it.
    pub fn start_languages(&self, path: &Path) -> Languages {
        self.start_on_chain(path, &mut Vec::new())
    }

    /// The languages active after `\include "text"` in `from`, given those active before it.
    pub fn languages_after_include(&self, from: &Path, text: &str, before: Languages) -> Languages {
        self.after_on_chain(from, text, before, &mut Vec::new())
    }

    /// The summary of an open file, or of a file on disk, read again when it has changed.
    fn summary(&self, path: &Path) -> Option<Rc<Summary>> {
        let key = canonical(path);
        if let Some(summary) = self.open.get(&key) {
            return Some(summary.clone());
        }
        let modified = fs::metadata(&key).ok()?.modified().ok();
        if let Some((when, summary)) = self.disk.borrow().get(&key)
            && modified.is_some()
            && *when == modified
        {
            return Some(summary.clone());
        }
        let text = fs::read_to_string(&key).ok()?;
        let tree = self.parser.borrow_mut().parse(&text, None)?;
        let summary = Rc::new(summarize(&tree, &text, path));
        self.disk
            .borrow_mut()
            .insert(key, (modified, summary.clone()));
        Some(summary)
    }

    /// `path` and every file it includes, directly or not, each once, by canonical path.
    fn closure(&self, path: &Path) -> Vec<(PathBuf, Rc<Summary>)> {
        let mut seen = HashSet::new();
        let mut files = Vec::new();
        self.walk(path, &mut seen, &mut |key, summary| {
            files.push((key.to_owned(), summary.clone()))
        });
        files
    }

    fn collect(&self, path: &Path, seen: &mut HashSet<PathBuf>, found: &mut Vec<Definition>) {
        self.walk(path, seen, &mut |_, summary| {
            found.extend(summary.definitions.iter().cloned())
        });
    }

    /// Visits `path` and the files it includes, skipping any already in `seen`, so a cycle ends.
    fn walk(
        &self,
        path: &Path,
        seen: &mut HashSet<PathBuf>,
        visit: &mut dyn FnMut(&Path, &Rc<Summary>),
    ) {
        let mut stack = vec![path.to_owned()];
        while let Some(path) = stack.pop() {
            let key = canonical(&path);
            if !seen.insert(key.clone()) {
                continue;
            }
            let Some(summary) = self.summary(&path) else {
                continue;
            };
            visit(&key, &summary);
            // Pushed in reverse, so they are visited in the order they are written.
            for step in summary.steps.iter().rev() {
                if let Step::Include(text) = step
                    && let Some(included) = self.resolve_include(&path, text)
                {
                    stack.push(included);
                }
            }
        }
    }

    fn start_on_chain(&self, path: &Path, chain: &mut Vec<PathBuf>) -> Languages {
        let key = canonical(path);
        if chain.contains(&key) {
            return None;
        }
        chain.push(key.clone());
        let mut reachable = HashSet::new();
        let mut files = Vec::new();
        for open in self.open.keys() {
            self.walk(open, &mut reachable, &mut |p, summary| {
                files.push((p.to_owned(), summary.clone()))
            });
        }
        let mut union = BTreeSet::new();
        let mut included = false;
        for (includer, summary) in &files {
            if !summary
                .steps
                .iter()
                .any(|s| matches!(s, Step::Include(t) if self.includes(includer, t, &key)))
            {
                continue;
            }
            included = true;
            let mut current = self.start_on_chain(includer, chain);
            for step in &summary.steps {
                match step {
                    Step::Language(name) => current = Some(BTreeSet::from([name.clone()])),
                    Step::Include(text) if self.includes(includer, text, &key) => match &current {
                        Some(languages) => union.extend(languages.iter().cloned()),
                        None => {
                            chain.pop();
                            return None;
                        }
                    },
                    Step::Include(text) => {
                        current = self.after_on_chain(includer, text, current, chain)
                    }
                }
            }
        }
        chain.pop();
        if included {
            Some(union)
        } else if path.extension().is_some_and(|e| e == "ily") {
            None
        } else {
            Some(BTreeSet::from([DEFAULT_LANGUAGE.to_owned()]))
        }
    }

    fn includes(&self, from: &Path, text: &str, key: &Path) -> bool {
        self.resolve_include(from, text)
            .is_some_and(|p| canonical(&p) == key)
    }

    fn after_on_chain(
        &self,
        from: &Path,
        text: &str,
        before: Languages,
        chain: &mut Vec<PathBuf>,
    ) -> Languages {
        let Some(path) = self.resolve_include(from, text) else {
            return before;
        };
        let key = canonical(&path);
        if chain.contains(&key) {
            return before;
        }
        let Some(summary) = self.summary(&path) else {
            return before;
        };
        chain.push(key);
        let mut current = before;
        for step in &summary.steps {
            current = match step {
                Step::Language(name) => Some(BTreeSet::from([name.clone()])),
                Step::Include(text) => self.after_on_chain(&path, text, current, chain),
            };
        }
        chain.pop();
        current
    }
}

/// `path` with `.` dropped and each `..` taking off the folder before it, without asking the
/// file system.
fn normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// The path with links resolved, so one file reached two ways is one key. A file that does not
/// exist yet keeps its name under its resolved folder.
fn canonical(path: &Path) -> PathBuf {
    if let Ok(resolved) = fs::canonicalize(path) {
        return resolved;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => fs::canonicalize(parent)
            .map(|p| p.join(name))
            .unwrap_or_else(|_| path.to_owned()),
        _ => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::time::Duration;

    use super::*;
    use crate::testing::{Scratch, document};

    fn names(definitions: &[Definition]) -> Vec<String> {
        let mut names: Vec<String> = definitions.iter().map(|d| d.name.clone()).collect();
        names.sort();
        names
    }

    #[test]
    fn summary_finds_every_definition_form() {
        let source = r#"melody = { c }
"quoted name" = { c }
verse.one = \lyricmode { la }
fn = #(define-music-function (sym m) (symbol? (ly:music? (make-music 'SequentialMusic))) m)
#(define (helper x) x)
#(define-public answer 42)
#(define-markup-command (frob layout props arg) (markup?) arg)
\paper { indent = 3\mm }
"#;
        let doc = document(None, source);
        let summary = summarize(&doc.tree, &doc.text, Path::new("/music/a.ly"));
        assert_eq!(
            names(&summary.definitions),
            [
                "answer",
                "fn",
                "frob",
                "helper",
                "melody",
                "quoted name",
                "verse"
            ]
        );
        let fun = summary.definitions.iter().find(|d| d.name == "fn").unwrap();
        assert_eq!(
            fun.signature.as_deref(),
            Some(
                &[
                    Arg {
                        pred: "symbol?".into(),
                        optional: false
                    },
                    Arg {
                        pred: "ly:music?".into(),
                        optional: true
                    },
                ][..]
            )
        );
        assert_eq!(
            fun.range,
            Range::new(
                lsp_types::Position::new(3, 0),
                lsp_types::Position::new(3, 2)
            )
        );
        assert!(
            summary
                .definitions
                .iter()
                .find(|d| d.name == "melody")
                .unwrap()
                .signature
                .is_none()
        );
    }

    #[test]
    fn include_cycle_terminates() {
        let dir = Scratch::new("index-cycle");
        let a = dir.write("a.ily", "\\include \"b.ily\"\nfromA = { c }\n");
        dir.write("b.ily", "\\include \"a.ily\"\nfromB = { d }\n");
        let index = Index::new(Vec::new());
        assert_eq!(
            names(&index.definitions_visible_from(&a)),
            ["fromA", "fromB"]
        );
        assert_eq!(index.start_languages(&a), None);
    }

    #[test]
    fn disk_file_reread_when_changed() {
        let dir = Scratch::new("index-reread");
        let defs = dir.write("defs.ily", "one = { c }\n");
        let main = dir.path().join("main.ly");
        let mut index = Index::new(Vec::new());
        index.update(&main, &document(Some(&main), "\\include \"defs.ily\"\n"));
        assert_eq!(names(&index.definitions_visible_from(&main)), ["one"]);
        std::fs::write(&defs, "two = { c }\n").unwrap();
        // The file system may keep one modification time for both writes.
        let later = SystemTime::now() + Duration::from_secs(10);
        File::options()
            .write(true)
            .open(&defs)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_eq!(names(&index.definitions_visible_from(&main)), ["two"]);
    }

    #[test]
    fn open_file_wins_over_disk() {
        let dir = Scratch::new("index-open");
        let defs = dir.write("defs.ily", "onDisk = { c }\n");
        let main = dir.path().join("main.ly");
        let mut index = Index::new(Vec::new());
        index.update(&main, &document(Some(&main), "\\include \"defs.ily\"\n"));
        index.update(&defs, &document(Some(&defs), "inEditor = { c }\n"));
        assert_eq!(names(&index.definitions_visible_from(&main)), ["inEditor"]);
        index.close(&defs);
        assert_eq!(names(&index.definitions_visible_from(&main)), ["onDisk"]);
    }

    #[test]
    fn includers_definitions_visible() {
        let dir = Scratch::new("index-includers");
        let main = dir.path().join("main.ly");
        let part = dir.write("part.ily", "{ \\tune }\n");
        let mut index = Index::new(Vec::new());
        index.update(
            &main,
            &document(Some(&main), "tune = { c }\n\\include \"part.ily\"\n"),
        );
        assert_eq!(names(&index.definitions_visible_from(&part)), ["tune"]);
    }

    #[test]
    fn resolution_order() {
        let dir = Scratch::new("index-order");
        let main = dir.path().join("main.ly");
        let own = dir.write("x.ily", "");
        let lib = dir.write("lib/x.ily", "");
        let shipped = dir.write("share/ly/x.ily", "");
        let mut index = Index::new(vec!["lib".into()]);
        index.set_datadir(Some(dir.path().join("share")));
        assert_eq!(index.resolve_include(&main, "x.ily"), Some(own.clone()));
        std::fs::remove_file(&own).unwrap();
        assert_eq!(index.resolve_include(&main, "x.ily"), Some(lib.clone()));
        std::fs::remove_file(&lib).unwrap();
        assert_eq!(index.resolve_include(&main, "x.ily"), Some(shipped.clone()));
        std::fs::remove_file(&shipped).unwrap();
        assert_eq!(index.resolve_include(&main, "x.ily"), None);
    }

    #[test]
    fn update_reports_outline_changes() {
        let main = Path::new("/music/main.ly");
        let mut index = Index::new(Vec::new());
        let mut update = |text: &str| index.update(main, &document(Some(main), text));
        assert!(update("a = { c }\n"));
        assert!(!update("a = { d }\n"));
        assert!(update("b = { d }\n"));
        assert!(update("\\include \"x.ily\"\nb = { d }\n"));
    }

    #[test]
    fn languages_follow_includes() {
        let dir = Scratch::new("index-languages");
        let main = dir.path().join("main.ly");
        let part = dir.write("part.ily", "{ c }\n");
        dir.write("english.ily", "\\language \"english\"\n");
        let mut index = Index::new(Vec::new());
        let nederlands = Some(BTreeSet::from([DEFAULT_LANGUAGE.to_owned()]));
        let english = Some(BTreeSet::from(["english".to_owned()]));
        assert_eq!(index.start_languages(&main), nederlands);
        assert_eq!(
            index.languages_after_include(&main, "english.ily", nederlands.clone()),
            english
        );
        assert_eq!(
            index.languages_after_include(&main, "missing.ily", nederlands.clone()),
            nederlands
        );
        index.update(
            &main,
            &document(
                Some(&main),
                "\\include \"english.ily\"\n\\include \"part.ily\"\n",
            ),
        );
        assert_eq!(index.start_languages(&part), english);
    }
}
