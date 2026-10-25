//! What a node spells in the source.

use tree_sitter::Node;

/// The source text of a node, or nothing when the tree is out of step with the text.
pub fn text<'s>(node: Node, source: &'s str) -> &'s str {
    source.get(node.byte_range()).unwrap_or_default()
}

/// The text a `string` node stands for: without its quotes, and with `\n`, `\t`, `\\`, `\'` and
/// `\"` read as the characters they stand for.
pub fn string(node: Node, source: &str) -> String {
    unquote(text(node, source))
}

/// The name a `command` node spells, without its backslash. `\"my var"` is `my var`. A single
/// character that is not a letter keeps it, since LilyPond names `\=` and `\^` that way.
pub fn command(node: Node, source: &str) -> String {
    let spelled = text(node, source);
    let name = spelled.strip_prefix('\\').unwrap_or_default();
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some('"'), _) => unquote(name),
        (Some(c), None) if !c.is_alphabetic() => spelled.to_owned(),
        _ => name.to_owned(),
    }
}

/// How a command named `name` is typed: with one backslash before it, unless the name holds its
/// own, as `\=` does.
pub fn spelled(name: &str) -> String {
    if name.starts_with('\\') {
        name.to_owned()
    } else {
        format!("\\{name}")
    }
}

fn unquote(quoted: &str) -> String {
    let inner = quoted.strip_prefix('"').unwrap_or(quoted);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        // The closing quote. A string the tree had to close has none.
        if c == '"' {
            break;
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(c @ ('\\' | '\'' | '"')) => out.push(c),
            Some(c) => {
                out.push('\\');
                out.push(c);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{document, first};

    #[test]
    fn escapes_read() {
        assert_eq!(unquote(r#""a\"b\\c\nd\q""#), "a\"b\\c\nd\\q");
        // A string the tree had to close still reads.
        assert_eq!(unquote(r#""open"#), "open");
    }

    #[test]
    fn command_names() {
        let doc = document(None, r#"{ \"my var" }"#);
        let quoted = first(doc.tree.root_node(), "command").unwrap();
        assert_eq!(command(quoted, &doc.text), "my var");
        let doc = document(None, r"{ \melody }");
        let plain = first(doc.tree.root_node(), "command").unwrap();
        assert_eq!(command(plain, &doc.text), "melody");
        let doc = document(None, r"{ c\= 1 }");
        let punctuation = first(doc.tree.root_node(), "command").unwrap();
        assert_eq!(command(punctuation, &doc.text), r"\=");
    }
}
