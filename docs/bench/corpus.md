# The comparison corpus

The grammar comparison needs a large body of LilyPond that LilyPond itself accepts, so that a parse error from either grammar is the grammar's fault and not the file's. This corpus is that body. It is gathered from four public sources, run through LilyPond 2.26.0 one file at a time, and sorted by whether LilyPond accepts it. The files carry the GPL or their own licences, so they stay on this machine under the git-ignored `bench/corpus/`. Only the result is committed: `bench/results/corpus-manifest.json` names every file with its size, its SHA-256 and its class, and holds no file content. The run described here took place on 2026-10-24 and took 49 s with 18 jobs.

## The counts

Of 2,915 files, 2,757 passed the check as they were and 17 more passed after `convert-ly` updated them, so the comparison corpus is 2,774 files. 141 were rejected and none timed out.

| Source | Files | Accepted | Converted | Rejected | Timeout |
| --- | ---: | ---: | ---: | ---: | ---: |
| regression | 2,237 | 2,181 | 0 | 56 | 0 |
| snippets | 425 | 423 | 0 | 2 | 0 |
| init | 62 | 56 | 0 | 6 | 0 |
| mutopia | 191 | 97 | 17 | 77 | 0 |
| total | 2,915 | 2,757 | 17 | 141 | 0 |

## Where the files come from

`cargo xtask corpus fetch` gathers everything into `bench/corpus/raw/`, one folder per source, and is safe to run again: a second run finds everything in place and downloads nothing.

The **regression** and **snippets** folders are `input/regression/` and `Documentation/snippets/` from the LilyPond 2.26.0 source release, `lilypond-2.26.0.tar.gz` from lilypond.org (20,130,301 bytes). The regression tests exercise one feature each, so they cover the grammar widely. The snippets are the examples printed in LilyPond's own documentation.

The **init** folder is the `.ly` files LilyPond loads at startup, copied from the installed LilyPond's `share/lilypond/2.26.0/ly/`, which is found by following the `lilypond` on `PATH`. They lean heavily on Scheme embedded in LilyPond, which the grammar has to handle.

The **mutopia** folder holds 306 files from the Mutopia Project, a library of public-domain scores typeset in LilyPond. They are the closest thing in the corpus to music written by ordinary users. `bench/mutopia.txt` lists every URL, all at MutopiaProject commit `2144afd6f52d56c5b6995b8b589ef1268b3139f0`, so the set never changes underneath the benchmark. The fetch refuses any URL that is not under that commit's `ftp/` tree or that contains an empty, `.` or `..` path part. The 306 files come from 64 folders in 62 distinct pieces by 26 composers, of which 191 are `.ly` files and are classified.

A fifth source, **lessons**, is copied from the folder named by `SWARA_LESSONS_DIR` when that variable is set. It was not set for this run, so the corpus holds no lesson files.

## How a file is classified

`cargo xtask corpus filter` checks every `.ly` file under `raw/` with the same command the language server uses, taken from the LilyPond check spike, except that it reads the file by name rather than from standard input:

```bash
LC_ALL=C lilypond -l WARNING -dinclude-settings=<abs>/check-settings.ly -I <abs dir> -o <abs tmp>/<n> <file>
```

The command runs from the file's own folder, with that folder on the include path, so `\include` statements resolve exactly as they do when the file is built. `check-settings.ly` is `crates/swara/assets/lilypond/check-settings.ly`, which stops LilyPond before it engraves anything. Each run gets 60 s before it is killed. The filter refuses to start unless `lilypond --version` reports GNU LilyPond 2.26.0.

Each file lands in one of four classes. A file the check passes, with exit status 0, is **accepted**. A file that fails is copied to a working file beside the original, `convert-ly -e` updates the copy to current syntax, and the copy is checked again. If it now passes, the file is **converted**, and the updated text is what goes into the corpus. If it still fails, it is **rejected**. A file whose check, or whose second check after conversion, runs past the limit is **timeout**. Accepted and converted files are copied into `bench/corpus/accepted/<source>/`, rejected ones into `rejected/`, and timeouts into `timeout/`, each keeping its path below its source folder. For a converted file the manifest records the SHA-256 of both the original and the updated text.

The filter reads every file as bytes, never as text, so a file that is not valid UTF-8 is classified like any other rather than skipped.

## Why files were rejected

The first `error:` line LilyPond printed for each of the 141 rejected files was read back after the run, and the reasons fall into a few groups. Most of them are a consequence of how the check works, or of what was fetched, rather than of a file being broken.

In **regression**, 45 of the 56 rejections are tests that declare, with `ly:expect-warning`, a warning they expect to provoke. Many of those warnings come from engraving, which the check never reaches, so LilyPond stops with a fatal error saying an expected warning did not arrive. The one such file tried by hand, `beam-unterminated.ly`, builds with exit status 0 when it is engraved in full. That this holds for all 45 is inferred, not measured. Of the rest, 7 stop on a Guile error, 2 on a check the test makes of its own bar number or beat structure, and 2 exit with an error but print no `error:` line.

In **init**, the 6 rejected files are meant to be loaded by LilyPond or by another init file and are not complete on their own: one uses `\maininput`, which is allowed only in init files, one calls `call-after-session` after the session has started, 2 stop on a Guile error and 2 on a syntax error. The 2 rejected **snippets** exit with an error but print no `error:` line.

In **mutopia**, 30 of the 77 rejections are files whose include files were never fetched: 26 parts of Dvořák's Seventh Symphony that all include `Common/Version.ly`, and one file each from four other pieces. The list in `bench/mutopia.txt` was drawn up folder by folder, and those include files sit in sub-folders of the pieces that were not drawn. Fetching the missing sub-folders of all 62 pieces would mean 4,857 more files and 6,953,820 more bytes, most of them from four large vocal works, which would change what the corpus represents, so the list was left as it is. Another 31 Mutopia files use commands that no longer exist, such as `\noTupletBracket`, `\compressFullBarRests` and `\encoding`, which `convert-ly` did not replace. Of those 31 and the 5 that use German note names such as `h` without declaring the language, 19 carry no `\version` statement, which leaves `convert-ly` no starting version to convert from. The last 11 stop on 6 Guile errors and 5 syntax or expression errors.

## Limitations

Only `.ly` files are classified. An `.ily` file is an include, meant to be read inside another file, so it is kept in the raw tree as context and never checked on its own, and it never enters the comparison corpus.

Conversion updates only the `.ly` file. Its includes are read as they are, so a Mutopia file whose own text converts cleanly but whose include file uses old syntax is still rejected.

A file the check accepts is valid as far as parsing and interpretation go, but it was never engraved, so an error that only engraving reveals would go unnoticed. For the comparison this is the right test, since a grammar sees exactly what the parser sees.

## Reproducing

From the repository root, with LilyPond 2.26.0 on `PATH`:

```bash
cargo xtask corpus fetch
cargo xtask corpus filter --jobs 18 --timeout-secs 60
```

`--jobs` defaults to the number of cores and `--timeout-secs` to 60. The filter clears `bench/corpus/accepted/`, `rejected/` and `timeout/` before it starts, rewrites the manifest when it finishes, and prints a progress line every 100 files.
