# Neovim facts the plugin relies on

Three behaviours of Neovim's Tree-sitter and filetype code decide how the plugin loads Swara's grammar. Each was checked on 2026-10-25 against Neovim 0.12.6 in `nvim --clean --headless`, with the grammar library a debug build of `swara-parser` names in `swara info`.

## The symbol a grammar library is loaded by

`vim.treesitter.language.add(lang, { path = ..., symbol_name = name })` puts `tree_sitter_` in front of `symbol_name` and looks that symbol up in the library. With `symbol_name = 'lilypond_swara'` the library loaded, and it exports `tree_sitter_lilypond_swara` and no other Tree-sitter symbol. With `symbol_name = 'nope'` the call failed with `dlsym(..., tree_sitter_nope): symbol not found`. So the `grammar_symbol` that `swara info` reports, `lilypond_swara`, is the right value to pass, and the server's `info.rs` keeps it.

## The shape of a language's symbol list

`vim.treesitter.language.inspect(lang)` returns a table with the keys `_wasm`, `abi_version`, `fields`, `metadata`, `state_count`, `supertypes` and `symbols`. Its `symbols` field is a map from each node kind to a boolean, `true` for a named kind and `false` for an anonymous one. Anonymous kinds keep their double quotes in the key, so the open brace is `symbols['"{"']`, and `symbols['{']` is `nil`. Swara's grammar has 197 entries, 95 of them anonymous, and `symbols.embedded_lilypond` is `true`. The check that a loaded `lilypond` language is Swara's therefore reads `inspect('lilypond').symbols.embedded_lilypond == true`.

## The filetype of LilyPond files

Neovim's own filetype detection already maps both `.ly` and `.ily` to the filetype `lilypond`, as `vim.filetype.match({ filename = 'a.ly' })` and `vim.filetype.match({ filename = 'a.ily' })` both answer. The plugin adds no filetype rule of its own.
