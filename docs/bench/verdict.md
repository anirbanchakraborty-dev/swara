# The grammar verdict

Swara's grammar, `tree-sitter-lilypond`, set out to be better than the existing LilyPond grammar for Tree-sitter, `nwhetsell/tree-sitter-lilypond`, and this page records how the two compare on the same corpus with the same harness. The answer is no, not yet. Swara's grammar is faster and catches nearly twice as many deliberately broken files, but it reports errors in 118 of the 2,774 files LilyPond accepts, where nwhetsell's reports them in 2. Of the three criteria the comparison sets, it passes two and fails the first.

## The criteria

The comparison was fixed before Swara's grammar was measured, in the grammar plan. Swara's grammar counts as better only if all three of these hold:

- Fewer accepted corpus files show any error than with the baseline.
- Its throughput in bytes per millisecond is at least the baseline's, where the baseline is timed with its embedded Scheme grammar included.
- At least as many broken files are caught within one line of where LilyPond reports the error.

## The numbers

| Measure | Swara | nwhetsell | Criterion |
| --- | ---: | ---: | --- |
| Accepted files parsed | 2,774 | 2,774 | |
| Accepted files with an error | 118 | 2 | fails |
| Throughput | 28,112 bytes/ms | 15,593 bytes/ms | passes |
| Broken files caught | 30 of 36 | 16 of 36 | passes |
| Median time per file | 15 µs | 25 µs | |
| Slowest file | 2.5 ms | 5.4 ms | |

Swara's grammar was measured at commit `ca12c11f5aa9205c116398253280037f0194ec3a` of `tree-sitter-lilypond`, on 2026-10-24, on the same laptop as the baseline. nwhetsell's figures are the baseline's, measured at commit `3d999a19c7b5ffb6e777ec6cbd56dc3a008bf169` and described in `baseline.md`. The full result, one line per file, is `bench/results/2026-10-24-swara-grammar.json`, and like the baseline it names each file by its corpus path and holds no file content. A second run on the same day gave 28,399 bytes/ms, so the throughput figure moves by about 1% between runs. An earlier measurement, at commit `55874acfcc4f9bc72e5bbfca3cf3ba59e6c9b540` before the grammar's review, is described under the accepted files below.

## Speed

Swara's grammar parses the corpus 1.80 times as fast as nwhetsell's pair of grammars. The comparison is fair in the way the criterion asks: Swara reads embedded Scheme into the same tree as the music around it, so its single parse does the work that nwhetsell splits between a LilyPond parse and a second pass over every injected Scheme range. Against nwhetsell's LilyPond grammar alone, about 25,000 bytes/ms with Scheme left unparsed, Swara's is still faster, though by a smaller margin. Its slowest file is `init/music-functions-init.ly` at 2.5 ms.

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

This is the criterion Swara's grammar fails. It reports an error in 118 accepted files, 4.3% of the corpus, against nwhetsell's 2. Of nwhetsell's two, Swara's grammar parses `regression/bom-mark.ly` cleanly and errs on `regression/other/display-lily-tests.ly` at the same line 98.

The first measurement, at commit `55874acfcc4f9bc72e5bbfca3cf3ba59e6c9b540`, found 432. The grammar's review then taught it the constructs behind most of those: a dotted context and object name passed to a command, as in `\omit Staff.TimeSignature`, a direction mark before a slur, tie, beam or hairpin, as in `f^(`, commands with their arguments inside a chord and inside a `\with` block, comma lists such as `beatStructure = 1,1`, negative values and a repeat type written as a string. That took the count from 432 to 118, and no file that parsed cleanly at the first commit shows an error now.

| Corpus folder | Files | With an error |
| --- | ---: | ---: |
| `regression/` | 2,181 | 100 |
| `snippets/` | 423 | 11 |
| `mutopia/` | 114 | 0 |
| `init/` | 56 | 7 |

The Mutopia scores matter most here, because they are the corpus files closest to what a person editing a piece of music writes, and none of the 114 shows an error. At the first commit, 15 of them did. The remaining errors sit in LilyPond's regression tests, which exercise rare corners of the language on purpose, in its documentation snippets and in its own startup files. The two grammars sit at opposite ends of one trade. nwhetsell's accepts nearly everything, so it shows almost no false errors, and 12 of the 20 broken files it misses report no error at all. Swara's rejects what it does not recognise, so it catches more real mistakes and also flags valid LilyPond it has not yet learned to read.

Grouping the 118 files by the construct on the line where the first error starts points at the cause, although it does not prove it, since an error can start a line after the slip that caused it. Every file falls in exactly one group:

| Construct on the first error line | Files | Example |
| --- | ---: | --- |
| a variable or `\etc` where a block or a fixed word is expected | 22 | `\new Staff \with \ctxmod`, `\alternative \alts` |
| a Scheme boolean written in capitals | 21 | `ragged-right = ##T` |
| an articulation shorthand written with a backslash | 20 | `c'4\3\^`, `e2\-` |
| embedded Scheme where the grammar does not expect it | 19 | `\new #context-type`, `$@(map` |
| a number with a trailing dot | 7 | `line-width = 4.\cm` |
| a `\tempo`, `\time` or `\voices` form | 6 | `\tempo Adagio`, `\tempo 4 = \tempoA` |
| a number argument inside a chord | 4 | `<c\rightHandFinger 1 e>` |
| a fraction or function call as a property value | 2 | `\set Score.timeSignatureFraction = 22/7` |
| something else | 17 | `\include` inside `\midi { }`, `\inherit-acceptability "TimeLess" "Staff"` |

The design expected some valid files to show errors whatever the grammar does, because parts of LilyPond's syntax depend on what a music function declares at run time. The number inside a chord is that kind of limit: whether the `1` in `<c\rightHandFinger 1 e>` belongs to the function or is an error depends on how many arguments the function takes, which only its definition says. Most of the other groups are fixed syntax that a grammar can learn. The grammar's review recorded the capital Scheme booleans, the backslash shorthands, the trailing dots and a variable after `\with` as known gaps rather than fixing them, because each is rare in a score, and none of them makes a Mutopia file fail. That reading is an inference from the groups, not a measured result, and the count can only be settled by closing the gaps and running the comparison again.

## Reproducing it

The grammar is built from its repository into the git-ignored `bench/third-party/` and compared with the same harness as the baseline, from the root of the `swara` repository:

```bash
tree-sitter build -o bench/third-party/swara-lilypond.dylib ../tree-sitter-lilypond
```

```bash
cargo run --release -p xtask -- compare --name swara --commit ca12c11f5aa9205c116398253280037f0194ec3a --lib bench/third-party/swara-lilypond.dylib --symbol tree_sitter_lilypond --out bench/results/2026-10-24-swara-grammar.json
```

No `--inject-lib` is passed, because Swara's grammar has no second grammar to inject. The harness, the corpus and the timing method are the baseline's, described in `baseline.md` and `corpus.md`.
