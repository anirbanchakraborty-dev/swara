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

## The server

The `swara` program and the `swara-parser` library reach every Swara user, so every crate in this section ships. The crates below were resolved together in a scratch project on 2026-10-24, so the whole tree was seen before any of them entered a `Cargo.toml`. Each one is added to `crates/swara/Cargo.toml` by the task that first uses it.

### tree-sitter-lilypond 0.1.0

Swara's own LilyPond grammar, from the sibling repository `anirbanchakraborty-dev/tree-sitter-lilypond`, under the same MIT licence. The server compiles it in to parse every open file, and `swara-parser` compiles the same grammar into the shared library Neovim loads, so highlighting and the server always agree on the tree. It is a path dependency until the grammar's v0.1.0 is released, and then a git dependency pinned to that release's commit. It pulls in `tree-sitter-language`, reviewed above, and needs `cc` to build its C parser.

### tree-sitter 0.27.1 and streaming-iterator 0.1.9

`tree-sitter` is reviewed above for `xtask`. The server uses its parser, its incremental re-parse after each edit and its query engine. In 0.27 the query engine hands back matches through the `StreamingIterator` trait, so code that walks query matches has to name the trait, which is why `streaming-iterator` is a direct dependency even though `tree-sitter` already pulls it in.

`streaming-iterator` is licensed MIT OR Apache-2.0. 0.1.9 was released on 2023-01-06 by Josh Stone, and the packaged crate is 18,482 bytes with no dependencies. It is old because it is finished: one trait and its adaptors.

### lsp-server 0.10.0

The language server scaffold rust-analyzer runs on, from the rust-analyzer repository. It reads and writes the protocol's JSON messages over standard input and output on two threads of its own and hands them to the server through channels, so the server's loop is ordinary synchronous code. It was chosen over `tower-lsp` because it needs no async runtime, which the design rules out.

Licence MIT OR Apache-2.0. 0.10.0 was published on 2026-07-16 by the Rust project's release account, and the packaged crate is 18,327 bytes. It pulls in `crossbeam-channel`, `log`, `serde`, `serde_derive` and `serde_json`.

### lsp-types 0.97.0

Rust types for the messages and structures of the Language Server Protocol, so a request such as `textDocument/completion` arrives as a typed value rather than loose JSON. Every request and notification Swara handles is defined in it: initialisation, opening, changing, saving and closing a file, diagnostics, completion, hover, definition, document symbols, formatting and command execution.

Licence MIT. 0.97.0 was released on 2024-06-04 by Markus Westerlind, the maintainer, and it is still the newest release. The packaged crate is 70,764 bytes. It pulls in `bitflags` 1.3.2, `fluent-uri` 0.1.4, `serde`, `serde_json` and `serde_repr`.

### crossbeam-channel 0.5.17

Channels that many threads can send into, with a `select!` that waits on several at once. `lsp-server` already depends on it, and the server names it directly because its loop selects over three sources: client messages, results from LilyPond child processes, and a timer built with `crossbeam_channel::at`.

Licence MIT OR Apache-2.0. 0.5.17 was published on 2026-09-05 by Taiki Endo, a crossbeam maintainer, and the packaged crate is 94,171 bytes. It pulls in `crossbeam-utils`.

### serde 1.0.229 and serde_json 1.0.154

`serde_json` is reviewed above, because `tree-sitter` needs it to build. The server uses it at run time to print `swara info`, to read the client's initialisation options and to read the cached command data. `serde`, with its `derive` feature, turns those JSON values into typed structures.

`serde` is licensed MIT OR Apache-2.0. 1.0.229 was published on 2026-07-18 by David Tolnay, its maintainer, and the packaged crate is 83,669 bytes. It pulls in `serde_core`, already in the tree, and, through `derive`, `serde_derive` with `proc-macro2`, `quote`, `syn` and `unicode-ident`. Those last five run inside the compiler while Swara builds, and none of their code is linked into the program.

### libc 0.2.191

The C library's functions, declared for Rust. On macOS and Linux the server starts each LilyPond run in a process group of its own and stops the whole group when a check is cancelled or runs too long, so a Ghostscript process LilyPond started never outlives it. Signalling a process group needs `killpg`, which the standard library does not offer. It is not built on Windows.

Licence MIT OR Apache-2.0. 0.2.191 was published on 2026-10-16 from the Rust project's `libc` repository, and it has no dependencies. The packaged crate is 855,401 bytes, almost all of it declarations for every platform Rust supports, of which Swara calls a handful.

### What the server adds to the tree

These crates are new to the tree beyond those in the `xtask` table above.

| Crate | Version | Licence | Needed by |
| --- | --- | --- | --- |
| bitflags | 1.3.2 | MIT/Apache-2.0 | lsp-types, fluent-uri |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | crossbeam-channel |
| fluent-uri | 0.1.4 | MIT | lsp-types |
| log | 0.4.34 | MIT OR Apache-2.0 | lsp-server |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | serde_derive, serde_repr, to build |
| quote | 1.0.47 | MIT OR Apache-2.0 | serde_derive, serde_repr, to build |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | serde, lsp-server, to build |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 | lsp-types, to build |
| syn | 3.0.9 | MIT OR Apache-2.0 | serde_derive, serde_repr, to build |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | proc-macro2, syn, to build |

Two of them are older than the rest, and both are held back by `lsp-types`. `bitflags` 1.3.2 is from 2021-08-16, the last of the 1.x line, which is finished rather than abandoned, and it has no dependencies. `fluent-uri` 0.1.4 is from 2023-03-11, and its author still maintains the crate, now at 0.4, but `lsp-types` 0.97 asks for 0.1. `unicode-ident` carries the Unicode licence for the character tables it is generated from, which is permissive.

Every licence in the tree is permissive and compatible with Swara's MIT licence, and `deny.toml` allows exactly these: Apache-2.0, ISC, MIT, Unicode-3.0, Unlicense and Zlib. The table is the output of `cargo tree -e normal,build` over the server's dependencies on 2026-10-24, with the licence each crate declares.
