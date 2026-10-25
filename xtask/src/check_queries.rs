//! `cargo xtask check-queries`: the plugin's copies of the grammar's queries match the grammar.

use std::path::Path;

/// Fails when a file under `queries/lilypond/` is missing or differs from the query the grammar
/// crate carries.
pub fn run(root: &Path) -> Result<(), String> {
    check(
        &root.join("queries/lilypond"),
        &[
            ("highlights", tree_sitter_lilypond::HIGHLIGHTS_QUERY),
            ("folds", tree_sitter_lilypond::FOLDS_QUERY),
            ("indents", tree_sitter_lilypond::INDENTS_QUERY),
            ("locals", tree_sitter_lilypond::LOCALS_QUERY),
        ],
    )
}

/// Compares each `<name>.scm` in `dir` with its text, byte for byte.
fn check(dir: &Path, queries: &[(&str, &str)]) -> Result<(), String> {
    let differ: Vec<String> = queries
        .iter()
        .filter(|(name, text)| {
            std::fs::read(dir.join(format!("{name}.scm")))
                .ok()
                .as_deref()
                != Some(text.as_bytes())
        })
        .map(|(name, _)| format!("{name}.scm"))
        .collect();
    if differ.is_empty() {
        return Ok(());
    }
    Err(format!(
        "query files missing or not the same as the grammar's: {} (in {})\n\
         copy them again from the tree-sitter-lilypond commit this repository is tested with",
        differ.join(", "),
        dir.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::check;

    const QUERIES: [(&str, &str); 2] = [
        ("highlights", "(note) @variable\n"),
        ("folds", "(block) @fold\n"),
    ];

    /// Runs `check` on a folder holding `files`, then removes the folder.
    fn check_files(name: &str, files: &[(&str, &str)]) -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("swara-queries-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (file, text) in files {
            std::fs::write(dir.join(format!("{file}.scm")), text).unwrap();
        }
        let result = check(&dir, &QUERIES);
        std::fs::remove_dir_all(&dir).unwrap();
        result
    }

    #[test]
    fn matching_queries_pass() {
        assert_eq!(check_files("same", &QUERIES), Ok(()));
    }

    #[test]
    fn one_byte_off_fails() {
        let files = [QUERIES[0], ("folds", "(block) @fold \n")];
        let error = check_files("byte", &files).unwrap_err();
        assert!(error.contains("folds.scm"), "{error}");
        assert!(!error.contains("highlights.scm"), "{error}");
    }

    #[test]
    fn missing_query_fails() {
        let error = check_files("missing", &QUERIES[..1]).unwrap_err();
        assert!(error.contains("folds.scm"), "{error}");
    }
}
