//! The formatter: spacing only, never a note, a comment, a string or a line break.
//!
//! The text is read as a run of tokens, which are the tree's leaves, with each string and comment
//! taken whole. Only the whitespace between two tokens changes. On one line it becomes a single
//! space, or stays empty where the tokens touch, since `c4-.` and `d'8~` belong together. At a line
//! break the line's indent is set from the braces around its first token. Inside Scheme the gaps
//! are kept, and a continuation line moves by as much as the line the Scheme starts on.

use std::io::{Read as _, Write as _};
use std::process::ExitCode;

use lsp_types::{Position, Range, TextEdit};
use tree_sitter::{Node, Parser, Tree};

use crate::line_index::{Encoding, LineIndex};

#[derive(Debug, PartialEq)]
pub enum FormatOutcome {
    Unchanged,
    Changed(String),
    /// The text is left as it is, for the reason given.
    Refused(&'static str),
}

const SYNTAX_ERRORS: &str = "the file has syntax errors, so swara left it as it is";
const NOT_SPACING: &str = "tidying would have changed more than spacing, so swara left it as it is";

/// Nodes taken as one token, so nothing inside them is respaced.
const WHOLE: &[&str] = &[
    "string",
    "scheme_string",
    "line_comment",
    "block_comment",
    "scheme_comment",
    "scheme_block_comment",
    "scheme_datum_comment",
];

pub fn format(text: &str, indent: usize, parser: &mut Parser) -> FormatOutcome {
    let Some(tree) = parser.parse(text, None) else {
        return FormatOutcome::Refused(SYNTAX_ERRORS);
    };
    if tree.root_node().has_error() {
        return FormatOutcome::Refused(SYNTAX_ERRORS);
    }
    let out = respace(text, &tree, indent);
    if out == text {
        FormatOutcome::Unchanged
    } else if same_music(&tree, text, &out, parser) {
        FormatOutcome::Changed(out)
    } else {
        FormatOutcome::Refused(NOT_SPACING)
    }
}

/// One edit per changed line. The formatter keeps every line break, so line `n` of `new` is line
/// `n` of `old`.
pub fn line_edits(old: &str, index: &LineIndex, new: &str, enc: Encoding) -> Vec<TextEdit> {
    if old.split('\n').count() != new.split('\n').count() {
        let end = index.byte_to_position(old, old.len(), enc);
        return vec![TextEdit::new(
            Range::new(Position::new(0, 0), end),
            new.to_owned(),
        )];
    }
    let mut edits = Vec::new();
    let mut start = 0;
    for (row, (was, now)) in old.split('\n').zip(new.split('\n')).enumerate() {
        if was != now {
            let end = index.byte_to_position(old, start + was.len(), enc);
            let range = Range::new(Position::new(row as u32, 0), end);
            edits.push(TextEdit::new(range, now.to_owned()));
        }
        start += was.len() + 1;
    }
    edits
}

/// `swara format`: standard input to standard output. A refusal passes the input through and
/// says why on standard error, so an editor that pipes a buffer through this never loses it.
pub fn stdin(indent: usize) -> ExitCode {
    let mut text = String::new();
    if let Err(error) = std::io::stdin().read_to_string(&mut text) {
        eprintln!("swara format: {error}");
        return ExitCode::FAILURE;
    }
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_lilypond::LANGUAGE.into())
        .expect("the grammar's ABI matches the tree-sitter crate");
    let out = match format(&text, indent, &mut parser) {
        FormatOutcome::Changed(out) => out,
        FormatOutcome::Unchanged => text,
        FormatOutcome::Refused(reason) => {
            eprintln!("swara format: {reason}");
            text
        }
    };
    let mut stdout = std::io::stdout().lock();
    match stdout
        .write_all(out.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("swara format: {error}");
            ExitCode::FAILURE
        }
    }
}

struct Token<'t> {
    node: Node<'t>,
    /// The outermost Scheme node the token is in.
    scheme: Option<Node<'t>>,
}

fn tokens<'t>(node: Node<'t>, scheme: Option<Node<'t>>, found: &mut Vec<Token<'t>>) {
    let scheme = scheme.or((node.kind() == "scheme").then_some(node));
    if node.child_count() == 0 || WHOLE.contains(&node.kind()) {
        if !node.byte_range().is_empty() {
            found.push(Token { node, scheme });
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        tokens(child, scheme, found);
    }
}

fn respace(text: &str, tree: &Tree, indent: usize) -> String {
    let mut found = Vec::new();
    tokens(tree.root_node(), None, &mut found);
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    // How far each line's indent moved, for the Scheme that continues below it.
    let mut moved = vec![0isize; text.split('\n').count()];
    let mut prev: Option<&Token> = None;
    for token in &found {
        let gap = &text[prev.map_or(0, |p| p.node.end_byte())..token.node.start_byte()];
        let scheme = token
            .scheme
            .filter(|_| prev.is_some_and(|p| p.scheme == token.scheme));
        if !gap.chars().all(|c| c.is_ascii_whitespace()) {
            out.push_str(gap);
        } else if let Some(prev) = prev.filter(|_| !gap.contains('\n')) {
            if scheme.is_some() {
                out.push_str(gap);
            } else if spaced(prev.node, token.node, gap.is_empty()) {
                out.push(' ');
            }
        } else {
            let lead = line_breaks(gap, &mut out);
            if let Some(scheme) = scheme {
                let by = moved[scheme.start_position().row];
                if by >= 0 {
                    out.extend(std::iter::repeat_n(' ', by as usize));
                    out.push_str(lead);
                } else {
                    out.extend(lead.chars().skip(by.unsigned_abs()));
                }
            } else {
                let width = depth(token.node) * indent;
                out.extend(std::iter::repeat_n(' ', width));
                moved[token.node.start_position().row] = width as isize - lead.len() as isize;
            }
        }
        out.push_str(&text[token.node.byte_range()]);
        prev = Some(token);
    }
    line_breaks(&text[prev.map_or(0, |p| p.node.end_byte())..], &mut out);
    out
}

/// Writes the line breaks in `gap`, as `\n` or `\r\n`, dropping the spaces before each, and gives
/// back what follows the last one: the next line's indent.
fn line_breaks<'g>(gap: &'g str, out: &mut String) -> &'g str {
    let mut rest = gap;
    while let Some(at) = rest.find('\n') {
        out.push_str(if rest[..at].ends_with('\r') {
            "\r\n"
        } else {
            "\n"
        });
        rest = &rest[at + 1..];
    }
    rest
}

/// Whether two tokens on one line are written with a space between them.
fn spaced(prev: Node, next: Node, touching: bool) -> bool {
    !touching
        || matches!(prev.kind(), "{" | "<<")
        || matches!(next.kind(), "}" | ">>")
        || prev.kind() == "bar_check"
        || next.kind() == "bar_check"
}

/// How many braces enclose the line that starts with `token`: every `{ }` or `<< >>` around it
/// whose opener is before it. A line that starts with a closer is outside that closer's pair.
fn depth(token: Node) -> usize {
    let at = token.start_byte();
    let mut depth = 0;
    let mut node = token.parent();
    while let Some(outer) = node {
        if encloses(outer, at) {
            depth += 1;
        }
        node = outer.parent();
    }
    depth
}

/// Whether `node` is a `{ }` or `<< >>` pair whose opener ends at or before `at` and whose closer
/// starts after it. The opener is among the first children, as in `{`, `\score {` and `\with {`,
/// and the closer is the last.
fn encloses(node: Node, at: usize) -> bool {
    let count = node.child_count();
    let opener = (0..count.min(3))
        .filter_map(|i| node.child(i))
        .find(|c| !c.is_named() && matches!(c.kind(), "{" | "<<"));
    let closer = node
        .child(count.saturating_sub(1))
        .filter(|c| !c.is_named() && matches!(c.kind(), "}" | ">>"));
    opener.is_some_and(|o| o.end_byte() <= at) && closer.is_none_or(|c| c.start_byte() > at)
}

/// Whether `after` holds the same tokens as `before`, and no more errors.
fn same_music(before: &Tree, before_text: &str, after: &str, parser: &mut Parser) -> bool {
    let Some(tree) = parser.parse(after, None) else {
        return false;
    };
    let (was, was_errors) = leaves(before, before_text);
    let (now, now_errors) = leaves(&tree, after);
    now_errors <= was_errors && was == now
}

/// Every token's text in order, with strings and comments whole, and the count of error and
/// missing nodes.
fn leaves<'s>(tree: &Tree, text: &'s str) -> (Vec<&'s str>, usize) {
    let mut texts = Vec::new();
    let mut errors = 0;
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            errors += 1;
        }
        let whole = WHOLE.contains(&node.kind());
        if (whole || node.child_count() == 0) && !node.byte_range().is_empty() {
            texts.push(&text[node.byte_range()]);
        }
        if (!whole && cursor.goto_first_child()) || cursor.goto_next_sibling() {
            continue;
        }
        loop {
            if !cursor.goto_parent() {
                return (texts, errors);
            }
            if cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;
    use FormatOutcome::{Changed, Refused, Unchanged};

    fn parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_lilypond::LANGUAGE.into())
            .expect("the grammar loads");
        parser
    }

    fn tidy(text: &str) -> FormatOutcome {
        format(text, 2, &mut parser())
    }

    fn changed(text: &str) -> String {
        match tidy(text) {
            Changed(out) => out,
            other => panic!("expected a change to {text:?}, got {other:?}"),
        }
    }

    #[test]
    fn indents_blocks() {
        assert_eq!(
            changed("\\score {\n{\nc4  d |e\n}\n}"),
            "\\score {\n  {\n    c4 d | e\n  }\n}"
        );
    }

    #[test]
    fn keeps_attached_tokens() {
        assert_eq!(changed("{c4-.  d'8~ d}"), "{ c4-. d'8~ d }");
        assert_eq!(tidy("{ c4-. d'8~ d a--b }\n"), Unchanged);
    }

    #[test]
    fn keeps_line_breaks() {
        assert_eq!(changed("{ c\n\n\n d   e }\n"), "{ c\n\n\n  d e }\n");
        assert_eq!(changed("{  \r\nc  d\r\n\t}\r\n"), "{\r\n  c d\r\n}\r\n");
        assert_eq!(changed("\n\n  { c }  \n\n"), "\n\n{ c }\n\n");
    }

    #[test]
    fn keeps_comments_and_strings() {
        assert_eq!(
            changed("\\header {\ntitle  =  \"a  b\"   %  x  \n}\n"),
            "\\header {\n  title = \"a  b\" %  x  \n}\n"
        );
        assert_eq!(
            changed("{\n%{  a\n      b  %}\nc\n}\n"),
            "{\n  %{  a\n      b  %}\n  c\n}\n"
        );
    }

    #[test]
    fn scheme_moves_as_block() {
        assert_eq!(
            changed("{\n#(define (f x)\n   (+ x  1))\n}\n"),
            "{\n  #(define (f x)\n     (+ x  1))\n}\n"
        );
        assert_eq!(
            changed("{\n      #(display\n          \"x\")\n}\n"),
            "{\n  #(display\n      \"x\")\n}\n"
        );
        // A line that starts inside a string is the string's own text.
        assert_eq!(
            changed("{\n#(display \"a\n   b\")\n}\n"),
            "{\n  #(display \"a\n   b\")\n}\n"
        );
    }

    #[test]
    fn with_block_indents() {
        assert_eq!(
            changed("\\new Staff \\with {\ninstrumentName = \"a\"\n} { c }\n"),
            "\\new Staff \\with {\n  instrumentName = \"a\"\n} { c }\n"
        );
        assert_eq!(
            changed("<<\n\\new Staff {c}\n>>\n\\markup {\n\\bold  a\n}\n"),
            "<<\n  \\new Staff { c }\n>>\n\\markup {\n  \\bold a\n}\n"
        );
    }

    #[test]
    fn refuses_on_syntax_error() {
        assert!(matches!(tidy("{ c"), Refused(_)));
    }

    #[test]
    fn idempotent() {
        for text in [
            "\\score {\n{\nc4  d |e\n}\n}",
            "\\new Staff \\with {\ninstrumentName = \"a\"\n} { c }\n",
            "{\n      #(display\n          \"x\")\n}\n",
            "<<{c}\\\\{d}>>\n",
        ] {
            let once = changed(text);
            assert_eq!(tidy(&once), Unchanged, "{once:?}");
        }
    }

    #[test]
    fn token_guard_catches_change() {
        let mut parser = parser();
        let before = "{ c4 d }";
        let tree = parser.parse(before, None).unwrap();
        assert!(same_music(&tree, before, "{ c4  d }", &mut parser));
        assert!(!same_music(&tree, before, "{ c4 e }", &mut parser));
        assert!(!same_music(&tree, before, "{ c4 d", &mut parser));
        let before = "\\header { title = \"a  b\" }";
        let tree = parser.parse(before, None).unwrap();
        assert!(!same_music(
            &tree,
            before,
            "\\header { title = \"a b\" }",
            &mut parser
        ));
    }

    /// Every leaf's text, and every string's and comment's, in order, read with a cursor rather
    /// than the formatter's own walk.
    fn leaf_texts(text: &str, parser: &mut Parser) -> Vec<String> {
        let tree = parser.parse(text, None).unwrap();
        let mut cursor = tree.walk();
        let mut found = Vec::new();
        loop {
            let node = cursor.node();
            let kind = node.kind();
            if node.child_count() == 0 || kind.ends_with("string") || kind.ends_with("comment") {
                found.push(text[node.byte_range()].to_owned());
            }
            if cursor.goto_first_child() || cursor.goto_next_sibling() {
                continue;
            }
            loop {
                if !cursor.goto_parent() {
                    return found;
                }
                if cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    fn lilypond_files(folder: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(folder) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                lilypond_files(&path, found);
            } else if path.extension().is_some_and(|e| e == "ly" || e == "ily") {
                found.push(path);
            }
        }
    }

    #[test]
    fn every_corpus_case_survives() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/corpus/accepted");
        if !root.is_dir() {
            return;
        }
        let mut files = Vec::new();
        lilypond_files(&root, &mut files);
        let mut parser = parser();
        let (mut changed, mut broken, mut guarded) = (0, 0, Vec::new());
        for path in &files {
            let Ok(text) = fs::read_to_string(path) else {
                continue;
            };
            match format(&text, 2, &mut parser) {
                Changed(out) => {
                    changed += 1;
                    assert_eq!(
                        leaf_texts(&text, &mut parser),
                        leaf_texts(&out, &mut parser),
                        "{}",
                        path.display()
                    );
                    assert_eq!(
                        text.matches('\n').count(),
                        out.matches('\n').count(),
                        "{}",
                        path.display()
                    );
                    assert_eq!(
                        format(&out, 2, &mut parser),
                        Unchanged,
                        "{}",
                        path.display()
                    );
                }
                Unchanged => {}
                Refused(SYNTAX_ERRORS) => broken += 1,
                Refused(_) => guarded.push(path.display().to_string()),
            }
        }
        eprintln!(
            "{} files: {changed} changed, {broken} refused for syntax errors, {} caught by the guard",
            files.len(),
            guarded.len()
        );
        assert!(guarded.is_empty(), "the guard caught {guarded:#?}");
    }
}
