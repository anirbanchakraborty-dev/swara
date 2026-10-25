# The nwhetsell grammar baseline

Swara's grammar is measured against the existing LilyPond grammar for Tree-sitter, `nwhetsell/tree-sitter-lilypond`, and this page records where that grammar stands before Swara's exists. The comparison asks three things of a grammar: whether it parses the files LilyPond accepts without reporting an error, whether it reports an error near the line LilyPond does when a file is broken, and how fast it parses. The run described here took place on 2026-10-24, on an Apple M5 Pro laptop running macOS 27, and took 1.7 s from start to finish.

## The numbers

| Measure | Result |
| --- | ---: |
| Accepted files parsed | 2,774 |
| Accepted files with an error | 2 |
| Broken files caught | 16 of 36 |
| Throughput, LilyPond and embedded Scheme together | 15,593 bytes/ms |
| Throughput, LilyPond grammar alone | about 25,000 bytes/ms |
| Median time per file, both grammars | 25 µs |
| Slowest file | 5.4 ms |

The full result, one line per file, is `bench/results/2026-10-24-baseline-nwhetsell.json`. It names each file by its path in the corpus and holds no file content.

## What was measured

The grammar measured is `nwhetsell/tree-sitter-lilypond` at commit `3d999a19c7b5ffb6e777ec6cbd56dc3a008bf169`, dated 2026-08-31, cloned into the git-ignored `bench/third-party/`. That repository holds two grammars: `lilypond`, for LilyPond itself, and `lilypond-scheme`, for the Scheme that LilyPond files embed after `#` and `$`. Its injection query, `queries/injections.scm`, marks the text of each embedded Scheme expression so that an editor parses it with the second grammar. Both grammars were built from their checked-in `src/parser.c`, which is generated for Tree-sitter ABI 14, with the `tree-sitter` 0.27.1 CLI:

```bash
tree-sitter build -o bench/third-party/nw-lilypond.dylib bench/third-party/tree-sitter-lilypond/lilypond
tree-sitter build -o bench/third-party/nw-lilypond-scheme.dylib bench/third-party/tree-sitter-lilypond/lilypond-scheme
```

A separate repository, `nwhetsell/tree-sitter-lilypond-scheme`, at commit `adaeff343863ffd697f204a8715f035dbf216a38`, dated 2025-04-12, was cloned too. Its README declares it obsolete in favour of the copy inside `tree-sitter-lilypond`, so that copy is the one measured, and the separate repository plays no part in the numbers.

`cargo xtask compare` loads each grammar from its library at run time and parses every file in `bench/corpus/accepted/`, the 2,774 files LilyPond 2.26.0 accepts (see `corpus.md`), then every file in `bench/broken/`. For each file it parses the whole text with the LilyPond grammar, runs the injection query over the tree, and parses each marked range with the Scheme grammar. A file has an error when any `ERROR` or `MISSING` node appears in any of those trees, and its first error line is the line on which the earliest such node starts.

Each file is timed twice: once for the LilyPond parse, and once for the injection pass, which is the query plus every Scheme parse. Each figure is the median of five runs after one warm-up run, on one thread, in a release build. The throughput in the summary divides the corpus's 3,171,751 bytes by the sum of both figures over every file, kept to the nanosecond. The figure for the LilyPond grammar alone comes from the per-file times in the JSON, which are rounded down to whole microseconds, so it is slightly high and is given only to the nearest thousand. A second run on the same day gave 15,573 bytes/ms, so run-to-run noise is around a tenth of a percent.

Injection is followed one level deep. The Scheme grammar has its own node for LilyPond embedded back inside Scheme, `#{ … #}`, but the repository's injection query covers only the step from LilyPond to Scheme, so LilyPond nested inside Scheme is treated as opaque text and any error inside it goes unseen. An editor that adds the reverse query would see more.

## Where it errs on accepted files

Two accepted files get an error. `regression/bom-mark.ly` has a UTF-8 byte-order mark partway through, on line 3, which LilyPond accepts with a warning and the grammar does not. `regression/other/display-lily-tests.ly` gets an error on line 98, an empty embedded-LilyPond block inside a test of how void music is displayed. The injected Scheme trees add no errors anywhere in the corpus.

The tree-sitter CLI, run over the same 2,774 files with the same library, reports the same two files with errors starting on the same two lines, which checks the harness's error detection against an implementation it shares no code with.

## The broken files

A broken file is caught when the grammar's first error line is within one line of the line LilyPond reports. The grammar catches 16 of the 36 files. It finds every unclosed `<<`, unclosed chord, unbalanced Scheme parenthesis, extra brace and stray `#}`, both figure-mode errors, both forms of `\new` without a context type, the unclosed `#{` in a top-level assignment, the unterminated string in a markup, and the open lyric block at the end of a file.

The 20 it misses fall into two kinds. In 12 the grammar reports no error at all, because its rules accept something LilyPond does not:

- both chord-mode errors, a dangling `:` and a `/` with no bass note
- both header fields with no value
- both `\paper` errors and both `\layout` errors
- both `\override` statements with no `=`
- both stray `>>`

In the other 8 the grammar does report an error, but it starts too early. Tree-sitter's error recovery wraps the damaged region in one `ERROR` node, and in these files that node reaches back past the delimiter that was never closed to the opening brace of the outermost block around it. Six of the 8 start on line 5, the first line of content, where that outer block opens: both unclosed braces, the open lyric block inside a score, both open markups and the unterminated block comment. In the block-comment file, for instance, the `%{` is on line 7, where LilyPond reports it, but the error starts at the `{` of the `\relative` block on line 5. The other 2, the unterminated string in lyrics and the unclosed `#{` inside a music function, start on line 1, because the `ERROR` node there covers the whole file from its first byte.

## The detection node

Swara's Neovim plugin decides whether the loaded `lilypond` grammar is Swara's by looking for the node type `embedded_lilypond`, which exists only in Swara's grammar. Neither of nwhetsell's grammars has a node of exactly that name, which was checked in both `lilypond/src/node-types.json` and `lilypond-scheme/src/node-types.json`. Both do have `scheme_embedded_lilypond` and `scheme_embedded_lilypond_text`, which contain the name as a substring, so the check has to compare whole node names. A substring search would mistake nwhetsell's grammar for Swara's.

## Reproducing

From the repository root, with the corpus built as `corpus.md` describes and the `tree-sitter` 0.27.1 CLI on `PATH`:

```bash
git clone https://github.com/nwhetsell/tree-sitter-lilypond bench/third-party/tree-sitter-lilypond
git -C bench/third-party/tree-sitter-lilypond checkout 3d999a19c7b5ffb6e777ec6cbd56dc3a008bf169
tree-sitter build -o bench/third-party/nw-lilypond.dylib bench/third-party/tree-sitter-lilypond/lilypond
tree-sitter build -o bench/third-party/nw-lilypond-scheme.dylib bench/third-party/tree-sitter-lilypond/lilypond-scheme
cargo run --release -p xtask -- compare --name nwhetsell/tree-sitter-lilypond --commit 3d999a19c7b5ffb6e777ec6cbd56dc3a008bf169 --lib bench/third-party/nw-lilypond.dylib --symbol tree_sitter_lilypond --inject-lib bench/third-party/nw-lilypond-scheme.dylib --inject-symbol tree_sitter_lilypond_scheme --inject-query bench/third-party/tree-sitter-lilypond/queries/injections.scm --out bench/results/2026-10-24-baseline-nwhetsell.json
```
