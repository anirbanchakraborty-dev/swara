//! Load a Tree-sitter grammar from a shared library at run time, so that the comparison can
//! measure any grammar without linking it into xtask.

use libloading::Library;
use std::path::Path;
use tree_sitter::Language;
use tree_sitter_language::LanguageFn;

/// A grammar and the library it came from. The language points into the library's parse tables,
/// so the library stays loaded for as long as the language lives; fields drop in order.
pub struct DynGrammar {
    pub language: Language,
    _library: Library,
}

impl DynGrammar {
    /// Loads `path` and calls `symbol`, the `const TSLanguage *tree_sitter_<name>(void)` every
    /// generated grammar exports.
    pub fn load(path: &Path, symbol: &str) -> Result<DynGrammar, String> {
        // SAFETY: loading runs the library's initialisers. The libraries given here are grammars
        // built by `tree-sitter build`, which have none beyond their static tables.
        let library =
            unsafe { Library::new(path) }.map_err(|e| format!("{}: {e}", path.display()))?;
        // SAFETY: the symbol has the signature above in every generated grammar.
        let function = unsafe { library.get::<unsafe extern "C" fn() -> *const ()>(symbol) }
            .map_err(|e| format!("{}: {e}", path.display()))?;
        // SAFETY: as above; the pointer it returns stays valid while `library` is loaded.
        let language = Language::new(unsafe { LanguageFn::from_raw(*function) });
        Ok(DynGrammar {
            language,
            _library: library,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_library_is_reported_by_path() {
        let err = DynGrammar::load(Path::new("/nonexistent/grammar.dylib"), "tree_sitter_x")
            .err()
            .expect("loading a missing file fails");
        assert!(err.contains("/nonexistent/grammar.dylib"), "{err}");
    }
}
