//! Swara's LilyPond grammar as a C shared library, which Neovim loads with
//! `symbol_name = 'lilypond_swara'`.

/// The grammar under a symbol of Swara's own, so it never clashes with another LilyPond parser
/// that exports `tree_sitter_lilypond`.
#[unsafe(no_mangle)]
pub extern "C" fn tree_sitter_lilypond_swara() -> *const () {
    // SAFETY: `LANGUAGE` wraps the generated `tree_sitter_lilypond`, which takes no arguments and
    // returns a pointer to static data.
    unsafe { tree_sitter_lilypond::LANGUAGE.into_raw()() }
}
