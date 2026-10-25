# The grammar verdict

Swara's grammar, `tree-sitter-lilypond`, set out to be better than the existing LilyPond grammar for Tree-sitter, `nwhetsell/tree-sitter-lilypond`, and this page records how the two compare on the same corpus with the same harness. The answer is no, not yet, though the gap is now small. Swara's grammar is faster and catches nearly twice as many deliberately broken files, but it reports errors in 6 of the 2,774 files LilyPond accepts, where nwhetsell's reports them in 2. Of the three criteria the comparison sets, it passes two and fails the first.

## The criteria

The comparison was fixed before Swara's grammar was measured, in the grammar plan. Swara's grammar counts as better only if all three of these hold:

- Fewer accepted corpus files show any error than with the baseline.
- Its throughput in bytes per millisecond is at least the baseline's, where the baseline is timed with its embedded Scheme grammar included.
- At least as many broken files are caught within one line of where LilyPond reports the error.

## The numbers

| Measure | Swara | nwhetsell | Criterion |
| --- | ---: | ---: | --- |
| Accepted files parsed | 2,774 | 2,774 | |
| Accepted files with an error | 6 | 2 | fails |
| Throughput | 26,112 bytes/ms | 15,593 bytes/ms | passes |
| Broken files caught | 30 of 36 | 16 of 36 | passes |
| Median time per file | 16 µs | 25 µs | |
| Slowest file | 2.5 ms | 5.4 ms | |

Swara's grammar was measured at commit `862c5c5861f5fca6fb881559318d36994d7891e2` of `tree-sitter-lilypond`, on 2026-10-25, on the same laptop as the baseline. nwhetsell's figures are the baseline's, measured at commit `3d999a19c7b5ffb6e777ec6cbd56dc3a008bf169` and described in `baseline.md`. The full result, one line per file, is `bench/results/2026-10-25-swara-grammar.json`, and like the baseline it names each file by its corpus path and holds no file content. The comparison ran five times while macOS background services were busy on the same machine, and the throughput ranged from 21,602 to 27,997 bytes/ms. The table gives the median run, which is the one saved. The error and broken-file counts were the same in all five runs. Two earlier measurements are described under the accepted files below, and the second of them is kept as `bench/results/2026-10-24-swara-grammar.json`.

## Speed

Swara's grammar parses the corpus 1.67 times as fast as nwhetsell's pair of grammars, and its slowest run was still 1.39 times as fast. The comparison is fair in the way the criterion asks: Swara reads embedded Scheme into the same tree as the music around it, so its single parse does the work that nwhetsell splits between a LilyPond parse and a second pass over every injected Scheme range. Against nwhetsell's LilyPond grammar alone, about 25,000 bytes/ms with Scheme left unparsed, the two are about level, since the spread between Swara's own runs is wider than the gap. The measurement at commit `ca12c11f5aa9205c116398253280037f0194ec3a` read 28,112 bytes/ms on a quieter machine, so the drop to 26,112 mixes any cost of the grammar's later changes with the load, and these runs cannot separate the two. The slowest file in the saved run is `mutopia/BeethovenLv/O76/Six_Variations_Opus76/Six_Variations_Opus76.ly` at 2.5 ms.

## Broken files

Swara's grammar catches 30 of the 36 broken files and nwhetsell's catches 16. Fifteen are caught by both, 15 only by Swara and 1 only by nwhetsell. Ten of the 15 that only Swara catches are among the 12 that nwhetsell misses by reporting no error at all: both chord-mode errors, the header field with no value at the end of its block, the override and `\remove` lines missing a part, the three settings missing their `=`, and the stray `>>` both inside music and at the top level. In the other five, all open blocks, nwhetsell reports an error several lines too early. Swara misses these six:

| File | LilyPond | Swara | nwhetsell | Why Swara misses it |
| --- | ---: | ---: | ---: | --- |
| `header-missing-value-before-field.ly` | 8 | none | none | `composer = "Traditional"` after an empty `subtitle =` reads as the note `composer` with an octave check, then a string, because any word can be a note name in the tree and the name check is left to the language server |
| `paper-missing-value.ly` | 8 | none | none | the same reading of `ragged-last =` as a note with an octave check |
| `unbalanced-scheme-in-layout.ly` | 8 | 5 | 8 | the unclosed `(` takes both closing braces into the Scheme list, and recovery wraps the whole `\score` block from line 5 in one error node |
| `unclosed-brace-before-score.ly` | 9 | 11 | 5 | `\score` inside the open melody reads as an ordinary command, which LilyPond rejects inside music, so nothing looks wrong until the end of the file |
| `unterminated-block-comment.ly` | 7 | 9 | 5 | a `%{` that never closes cannot be a block comment, so it reads as a one-character line comment and an opening brace, and that brace is reported missing on the last line |
| `unterminated-string-lyrics.ly` | 8 | 5 | 1 | a `"` that never closes cannot be a string, and recovery wraps everything from the `<<` on line 5 in one error node |

## Accepted files

This is the criterion Swara's grammar fails. It reports an error in 6 accepted files, 0.2% of the corpus, against nwhetsell's 2. Of nwhetsell's two, Swara's grammar parses `regression/bom-mark.ly` cleanly and errs on `regression/other/display-lily-tests.ly` at the same line 98.

The count came down in three steps. The first measurement, at commit `55874acfcc4f9bc72e5bbfca3cf3ba59e6c9b540`, found 432. The grammar's review then taught it the constructs behind most of those: a dotted context and object name passed to a command, as in `\omit Staff.TimeSignature`, a direction mark before a slur, tie, beam or hairpin, as in `f^(`, commands with their arguments inside a chord and inside a `\with` block, comma lists such as `beatStructure = 1,1`, negative values and a repeat type written as a string. That took the count to 118, at commit `ca12c11f5aa9205c116398253280037f0194ec3a`. A third round worked through those 118 by the construct on each file's first error line, and the grammar now reads the forms behind them: a Scheme boolean written in capitals, as in `##T`, a command of one punctuation character, as in `c'4\^`, a variable or `\etc` where a block or a fixed word was expected, embedded Scheme as a context type and a spliced list such as `$@(list ...)`, a number with a trailing dot before a unit, as in `4.\cm`, `\tempo` with a bare word, a variable or Scheme, a number argument inside a chord, and commands and includes inside `\header`, `\midi` and `\layout` blocks. Where a markup ends is now settled by whichever reading parses, because the same words end a markup in one file and carry on in the next. That took the count from 118 to 6, and no file that parsed cleanly at either earlier commit shows an error now.

| Corpus folder | Files | With an error |
| --- | ---: | ---: |
| `regression/` | 2,181 | 2 |
| `snippets/` | 423 | 3 |
| `mutopia/` | 114 | 0 |
| `init/` | 56 | 1 |

The Mutopia scores matter most here, because they are the corpus files closest to what a person editing a piece of music writes, and none of the 114 shows an error. At the first commit, 15 of them did. The six remaining errors sit in LilyPond's regression tests, its documentation snippets and one of its startup files. The two grammars sit at opposite ends of one trade. nwhetsell's accepts nearly everything, so it shows almost no false errors, and 12 of the 20 broken files it misses report no error at all. Swara's rejects what it does not recognise, so it catches more real mistakes and also flags the few valid forms it has not learned to read.

Each of the six files has one construct on its first error line, and they fall in four groups:

| Construct on the first error line | Files | Example |
| --- | ---: | --- |
| a character or a reader syntax that the file defines while it runs | 3 | `@c8` after `"@" =` defines a music function, `##[` after `read-hash-extend` |
| Scheme as the name of a setting inside a `\with` block | 1 | `#(string-append ...) = ...` |
| music in braces as a function argument inside a `\with` block | 1 | `\scaleDurations 1/4 { ... }` |
| a duration with a multiplier as a function argument in an override | 1 | `\musicLength 1*1/40` |

The design expected some valid files to show errors whatever the grammar does, because parts of LilyPond's syntax depend on what a file defines at run time. The first group is that kind of limit. Two of its files, copies of one documentation snippet in two folders, define a music function named `@` and then write it before notes with no backslash, and the third defines a new `#[` form for Scheme's reader. A grammar reads the text before any of it runs, so it cannot know these forms exist, and nwhetsell's errs on the third at the same line. The other three groups are fixed syntax that a grammar could learn. Each appears once in the corpus and never in a Mutopia score, so they are left as known gaps for a later release.

## Reproducing it

The grammar is built from its repository into the git-ignored `bench/third-party/` and compared with the same harness as the baseline, from the root of the `swara` repository:

```bash
tree-sitter build -o bench/third-party/swara-lilypond.dylib ../tree-sitter-lilypond
```

```bash
cargo run --release -p xtask -- compare --name swara --commit 862c5c5861f5fca6fb881559318d36994d7891e2 --lib bench/third-party/swara-lilypond.dylib --symbol tree_sitter_lilypond --out bench/results/2026-10-25-swara-grammar.json
```

No `--inject-lib` is passed, because Swara's grammar has no second grammar to inject. The harness, the corpus and the timing method are the baseline's, described in `baseline.md` and `corpus.md`.
