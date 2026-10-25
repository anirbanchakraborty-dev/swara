//! The library Neovim loads exports Swara's grammar under a symbol of its own.

use libloading::Library;
use std::path::PathBuf;
use tree_sitter::Language;
use tree_sitter_language::LanguageFn;

#[cfg(target_os = "macos")]
const LIBNAME: &str = "libswara_parser.dylib";
#[cfg(windows)]
const LIBNAME: &str = "swara_parser.dll";
#[cfg(not(any(target_os = "macos", windows)))]
const LIBNAME: &str = "libswara_parser.so";

/// Integration tests run from `target/<profile>/deps`. The library Cargo builds for them stays
/// there, and `cargo build` also copies it one folder up.
fn library() -> PathBuf {
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    [deps.join(LIBNAME), deps.parent().unwrap().join(LIBNAME)]
        .into_iter()
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| deps.join(LIBNAME))
}

#[test]
fn exported_symbol_gives_abi_15() {
    let path = library();
    assert!(path.is_file(), "{} is missing", path.display());
    // SAFETY: the library is this crate's own build, which has no initialisers of its own.
    let library = unsafe { Library::new(&path) }.unwrap();
    // SAFETY: the symbol has the signature of every generated grammar's language function.
    let function =
        unsafe { library.get::<unsafe extern "C" fn() -> *const ()>("tree_sitter_lilypond_swara") }
            .unwrap();
    // SAFETY: as above; the pointer stays valid while `library` is loaded, which outlives `language`.
    let language = Language::new(unsafe { LanguageFn::from_raw(*function) });

    assert_eq!(language.abi_version(), 15);
    let id = language.id_for_node_kind("embedded_lilypond", true);
    assert_ne!(id, 0, "embedded_lilypond is not a node of this grammar");
    assert!(language.node_kind_is_named(id));
}
