# Third-party crates

Every crate Swara depends on is reviewed before it is added: its licence, who maintains it and how recently, and what it pulls in behind it. This file is the record of those reviews. A crate that is not listed here should not appear in any `Cargo.toml`.

## xtask

`xtask` is the repository's own build and measurement tool. It is never published or shipped (`publish = false`), so nothing below reaches a Swara user.

### libloading 0.9.0

Loads a shared library at run time and looks up a symbol in it. The comparison harness, `cargo xtask compare`, uses it to load a Tree-sitter grammar built by `tree-sitter build`, so that any grammar can be measured without being linked into `xtask`. The server plan uses it again as a dev-dependency, to open the built grammar library in a test.

Licence ISC. Maintained by Simonas Kazlauskas, the crate's author. 0.9.0 was released on 2025-11-05, and the crate has been downloaded about 590 million times. The packaged crate is 33,816 bytes. On macOS and Linux it pulls in only `cfg-if`, and on Windows it pulls in `windows-link` instead. Every call into it is `unsafe`, because loading a library runs that library's code, so the harness loads only grammars it has just built from a pinned commit.

### tree-sitter 0.27.1

The official Rust bindings to the Tree-sitter parsing library, from the Tree-sitter project itself. It compiles the C runtime from source with `cc`, so the parser, the tree walk and the query engine used by the harness are the same code every editor that embeds Tree-sitter runs. 0.27.1 matches the `tree-sitter` 0.27.1 CLI that the grammar is generated with, and it reads grammars of ABI 13 to 15.

Licence MIT. Released on 2026-10-08 by the Tree-sitter maintainers. The packaged crate is 230,064 bytes. At run time it pulls in `regex` (with `regex-automata`, `regex-syntax`, `aho-corasick` and `memchr`), `streaming-iterator` and `tree-sitter-language`. To build, it needs `cc` (with `find-msvc-tools` and `shlex`) and `serde_json` (with `indexmap`, `hashbrown`, `equivalent`, `foldhash`, `itoa`, `serde_core` and `zmij`). The optional `wasm` and `bindgen` features are left off, so neither Wasmtime nor bindgen is built.

### tree-sitter-language 0.1.9

A one-type crate from the Tree-sitter project that holds `LanguageFn`, the wrapper around a grammar's exported function. `tree-sitter` depends on it but does not re-export the type, and `LanguageFn::from_raw` is the only way to turn a function found by `libloading` into a `tree_sitter::Language`, so `xtask` names it directly.

Licence MIT. Released on 2026-10-08 alongside `tree-sitter` 0.27.1. The packaged crate is 5,297 bytes and it has no dependencies.

### What the three pull in

| Crate | Version | Licence | Needed by |
| --- | --- | --- | --- |
| aho-corasick | 1.1.5 | Unlicense OR MIT | regex |
| cc | 1.10.0 | MIT OR Apache-2.0 | tree-sitter, to build |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | libloading |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | indexmap |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 | cc |
| foldhash | 0.2.0 | Zlib | serde_json |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | indexmap |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | serde_json |
| itoa | 1.0.18 | MIT OR Apache-2.0 | serde_json |
| memchr | 2.8.3 | Unlicense OR MIT | regex, aho-corasick, serde_json |
| regex | 1.13.1 | MIT OR Apache-2.0 | tree-sitter |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | regex |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | regex |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | serde_json |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | tree-sitter, to build |
| shlex | 2.0.1 | MIT OR Apache-2.0 | cc |
| streaming-iterator | 0.1.9 | MIT OR Apache-2.0 | tree-sitter |
| zmij | 1.0.23 | MIT | serde_json |

Every licence in the tree is permissive and compatible with Swara's MIT licence. The table is the output of `cargo tree -p xtask -e normal,build` on 2026-10-24, with the licence each crate declares.
