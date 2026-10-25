# Speed

The design sets a speed target for each thing the server does while someone types, and this page records how the server measures against each one. The figures come from `crates/swara/tests/speed.rs`, which drives a release build of `swara lsp` over its standard input and output as an editor would, against the real LilyPond and the 100-bar test song `bench/song-100.ly`. Every target was met. The run described here took place on 2026-10-25 with LilyPond 2.26.0 (Guile 3.0) and Rust 1.98.0, on the machine described in `../spikes/speed-basis.md`.

## Results

Each figure is the median of 20 timed runs taken after one warm-up run, with the fastest and the slowest beside it. Times are wall clock, in milliseconds, measured by the test client from the moment it writes a message to the moment it has read and parsed the answer it waits for.

| Measurement | Median | Fastest | Slowest | Target |
| --- | --- | --- | --- | --- |
| Ready to answer | 2.3 | 2.1 | 2.5 | under 100 |
| Instant checks after an edit | 1.2 | 1.0 | 2.3 | under 50 |
| LilyPond check after the last edit | 551.2 | 538.3 | 569.7 | under 600 |
| Tidy through the language server | 1.1 | 1.0 | 1.2 | under 10 |
| Tidy through `swara format` | 2.8 | 2.4 | 3.2 | under 10 |
| Completion after `\` | 1.1 | 1.1 | 1.2 | under 20 |
| PDF after save, 4-bar lesson | 266.3 | 259.7 | 277.6 | about 300 |
| PDF after save, 100-bar song | 1,310.5 | 1,264.5 | 1,475.2 | about 1,500 |

The LilyPond check is the one target with little room. Of its 551 ms, 330 ms is the pause the server waits for typing to stop, which leaves about 221 ms for LilyPond to read the text and report what it finds. That matches the 217 ms that `../spikes/lilypond-check.md` measured for the same command run by hand, so the server adds almost nothing of its own. The margin is 49 ms, and a slower machine or a song much longer than 100 bars could use it up.

The two PDF times are LilyPond's engraving and little else. The song's median of 1,310 ms sits beside the 1,311 ms that `speed-basis.md` measured for LilyPond alone on the same file, on a different day, so the server's share of a build is smaller than the spread between runs.

## How each figure is taken

Ready is the time from starting the server process to its reply to `initialize`, together with the `initialized` notification the client writes straight after. The command data LilyPond provides is in the cache before the timed runs start, as it is on every start after the first, although the server answers `initialize` without waiting for that data in any case.

Instant checks time a one-character edit on the song to the publish of diagnostics for that edit. The edit types a closing brace over the first note, and the next edit types the note back. The brace makes the parse report an error and the note clears it, so each publish shows by its errors which text it checked. The LilyPond check is switched off for this test, so that none of its publishes can be taken for an instant one.

The LilyPond check times the last edit to the publish that carries LilyPond's own messages for the edited text, the 330 ms pause included. The song carries one added line, `#(ly:warning "speed 0")`, and each edit changes its digit. An edit moves the earlier check's messages along with the text rather than clearing them, so only the publish that names the new digit can come from the new check.

Tidy times a `textDocument/formatting` request on the song. It separately times a fresh `swara format` process that reads the song on standard input and writes the result, with the start and exit of the process included.

Completion times a request just after a backslash typed on its own line at the top of the melody. The reply offers 728 items, and the time includes the client parsing all of them from JSON.

PDF after save times a save to the server's `swara/buildFinished` report that the PDF is in place beside the file. The 4-bar lesson is the song cut to its first four bars and four lyric lines, which is byte for byte what `cargo xtask gen-song --bars 4` writes. LilyPond's engraving sets the floor for these two, so their targets are approximate and the test fails only when a median exceeds its target by half again.

## Reproducing

The timing tests compile only into a release build and are marked ignored, so the ordinary test run skips them. They run one at a time, because a build running beside a timing would measure the build, and the whole set takes about 46 s.

```bash
cargo test -p swara --release --test speed -- --include-ignored --nocapture
```
