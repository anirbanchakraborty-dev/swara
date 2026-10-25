//! Records which grammar commit this build compiles in, for `swara info` and `:checkhealth`.

use std::path::Path;

fn main() {
    let lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    println!("cargo::rerun-if-changed={}", lock.display());
    let text = std::fs::read_to_string(&lock).unwrap_or_default();
    println!("cargo::rustc-env=SWARA_GRAMMAR_REV={}", grammar_rev(&text));
}

/// The commit after `#` in the grammar package's `source` line, or `local` when the grammar is a
/// path dependency, which has no source line.
fn grammar_rev(lock: &str) -> String {
    lock.split("[[package]]")
        .find(|package| package.lines().any(|line| line.trim() == r#"name = "tree-sitter-lilypond""#))
        .and_then(|package| package.lines().find_map(|line| line.trim().strip_prefix("source = \"git+")))
        .and_then(|source| source.trim_end_matches('"').rsplit_once('#'))
        .map_or_else(|| "local".to_owned(), |(_, rev)| rev.to_owned())
}
