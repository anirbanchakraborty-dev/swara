# LilyPond check spike

The server checks a file each time typing pauses, and the design gives that check 0.6 s from the last keystroke to the first message, of which 0.33 s is the pause itself. A full LilyPond build of the 100-bar test song takes about 1.3 s, so the check cannot be a build. This spike asked whether LilyPond can be made to parse and interpret a file, reporting what it finds, without laying out a single page, and whether it can read text that has not been saved. Both answers are yes. Everything here was measured on 2026-10-24 with LilyPond 2.26.0 (Guile 3.0) and hyperfine 2.0.0, on the machine described in `speed-basis.md`.

## The verdict

Candidate B wins. It reports every message that does not need engraving, at the right line, in 219 ms on the 100-bar song, inside the 0.27 s budget. It is now `crates/swara/assets/lilypond/check-settings.ly`, which the server embeds. The command the server runs, with the buffer's text on stdin, is this one argument list:

```bash
LC_ALL=C lilypond -l WARNING -dinclude-settings=<abs>/check-settings.ly -I <abs dir> -o <abs tmp>/<stem> -
```

`<abs dir>` is the folder of the file being edited, so that its includes resolve exactly as they do when the saved file is built. `<abs tmp>` is a temporary folder; nothing is written there, because the settings file never asks LilyPond for output, but `-o` keeps any future output away from the user's folder. Timed exactly as written, with the song on stdin, the command has a median of 217 ms over ten runs (215 ms to 222 ms).

## How the candidates work

LilyPond hands every finished book to a Scheme procedure called `toplevel-book-handler`, and the file passed with `-dinclude-settings` is read before the user's file, so redefining that procedure there changes what happens to every score. The final book step still runs after a syntax error, which is why a check of this kind reports interpretation warnings even in a file that does not parse cleanly.

Candidate A walks the book and interprets each score's music with `ly:run-translator` against a MIDI output definition: the score's own `\midi` block if it has one, the default otherwise. Interpretation is where bar checks are tested, so they are reported; engraving never starts.

Candidate B walks the book the same way but interprets each score under its own layout definition, cloned, with every engraver taken out of every context. Translators that keep time, such as `Timing_translator`, stay, so bar checks are still tested. An output definition is recognised by its Score context: `Score_engraver` marks a layout and `Score_performer` marks MIDI.

## The seeded test

`tests/fixtures/lilypond/seeded.ly` is the 100-bar song with five problems planted without moving any line: the Scheme error `#(car '())` on line 3, a bar one eighth short on line 20 (the next bar is one eighth long, so only one bar check fails), the unknown command `\fooBar` in the lyrics on line 150, a stray `}` on line 210, and a tablature note below the low string, `e,,4`, appended to the tab staff's music on line 216. `seeded.expect` lists them as `LINE:severity:substring`, and `spikes/lilypond-check/run.sh check <configuration>` compares a run against it.

An earlier placement of `\fooBar` on a blank line at the top level produced a second, knock-on error on the following line, because the parser reads an unknown command there as the start of an assignment. Inside a lyric block the same command produces exactly one message, so the fixture puts it there.

| Configuration | Messages found | Exit status | Files written | Median on the song |
| --- | --- | --- | --- | --- |
| Full build | 5 of 5 | 1 | PDF and MIDI | 1,329 ms |
| `-dno-print-pages` | 5 of 5 | 1 | MIDI | 919 ms |
| Candidate A | 4 of 5 | 1 | none | 237 ms |
| Candidate B | 4 of 5 | 1 | none | 219 ms |

The message both candidates miss is the tablature one, as expected, because only engraving looks for a string to put the note on. On the clean song both candidates print nothing and exit 0. Medians come from `run.sh time`, five runs after one warm-up; an earlier run of the same commands agreed within 10 ms on every row. An empty file takes 153 ms, so most of a check's time is LilyPond starting up.

## Why B and not A

Speed alone favours B by 18 ms, but correctness decides it. A score that defines its own context in `\layout`, such as a voice renamed `Melody`, makes candidate A print a false warning, `cannot create context: Melody`, because the MIDI definition knows nothing of contexts defined for the page. Candidate B uses the score's own layout definition and stays silent.

## What only a full build reports

Any message an engraver produces is outside the check. Besides the tablature string, a probe found three common ones: `unterminated slur`, `unterminated tie` and `unterminated crescendo`. Programming errors that arise during layout, such as the cyclic dependency in the `programming-error` fixture, are also full-build only. The build that runs on save reports all of these, so they reach the user a moment later rather than not at all.

## Unsaved text and includes

A main file that includes `part.ily` was fed on stdin with `-I` naming its folder, from a working directory elsewhere. The include resolved, with no `cannot find file`. Messages in the main text are named `-` and messages in the include are named by its absolute path. The recording is `stdin-include.stderr`, so no temporary-file fallback is needed.

## The message fixtures

Each case is a small `.ly` file beside the `.stderr` file recorded from it, in `tests/fixtures/lilypond/`. All of them were reproduced from real LilyPond runs, and none is synthetic. `tab-no-string` and `programming-error` were recorded with a full build, because only engraving reports them; `seeded.full.stderr` is the full build of the seeded file and `seeded.check.stderr` its check. Every other case was recorded with the check command and its text on stdin. `spikes/lilypond-check/record.sh` re-records them all, writes the fixtures folder as `/fixture` and the scratch folder as `/scratch` so that no recording depends on where the repository lives, and gives identical files when run twice.

The recordings pin down several facts the server's message parser has to honour.

- Columns are 1-based and counted in characters, not bytes. In `utf8-column`, the brace after `"গান"` is reported at column 15, where counting UTF-8 bytes would give 21.
- A tab advances the column to the next multiple of eight. In `tab-column`, the brace after eleven characters and a tab is reported at column 17, not 13.
- Most messages are followed by two lines of source: the line up to the error column, then the rest of it indented to that column. Some carry more: a Guile error adds Guile's own message on a line of its own.
- `no-version` has a line number but no column: `-:1: warning: no \version statement found`, followed by several lines of advice.
- `no-location` has neither: `warning: no such program option: such-option`. LilyPond read the `no-` of `no-such-option` as a negation, which is itself worth knowing.
- `reader-error` produces three errors, and after the first of them a located line with no severity word: `-:2:13: Unknown # object: "#<"`.
- `programming-error` is followed by a located `continuing, cross fingers` line, also without a severity word.
- `unknown-command` inside music produces two errors at the same place: `unknown command` and then `string outside of text script or \lyricmode`.
- A failed bar check in music that two staves share is reported once per staff, with the same location each time.
- `include-error` reports `cannot find file`, and its second line lists the include search path.
- A run that has any error ends with `fatal error: failed files: "-"`, or the file's name when it was not read from stdin.
