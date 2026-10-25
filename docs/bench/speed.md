# Speed

The design sets a speed target for each thing Swara does while someone types, builds or plays a score, and this page records how Swara measures against each one. The server's figures come from `crates/swara/tests/speed.rs`, which drives a release build of `swara lsp` over its standard input and output as an editor would, against the real LilyPond and the 100-bar test song `bench/song-100.ly`. The plugin's two, colours and playback, come from the plugin's own tests in headless Neovim. Every target was met. The runs described here took place on 2026-10-25 with LilyPond 2.26.0 (Guile 3.0), Rust 1.98.0, Neovim 0.12.6 and FluidSynth 2.6.1, on the machine described in `../spikes/speed-basis.md`.

## Results

### The server

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

### The plugin

Colours were timed in five runs of the plugin's colour test, each taking the median of 20 edits. Playback was timed in five runs of the real playback test, each playing an eight-note scale twice. The figures are the medians of the five runs, with the fastest and the slowest run beside them, in milliseconds.

| Measurement | Median | Fastest | Slowest | Target |
| --- | --- | --- | --- | --- |
| Colours after a keystroke | 0.19 | 0.19 | 0.20 | under 16 |
| Play, first time, to the `swara play` start | 1,057.6 | 1,040.0 | 1,217.5 | about 1,500 to first sound |
| Play, later, to the `swara play` start | 1.6 | 1.5 | 1.8 | under 200 to first sound |
| `swara play` start to its first note | 2.2 | 2.1 | 2.5 | none of its own |

Colours have the widest margin of any target, about 80 times. The 16 ms budget is one frame at 60 frames a second. Tree-sitter reparses incrementally, reusing every part of the old tree that an edit did not touch, which is why a 100-bar song costs so little.

The design times playback from the command to the first sound, and the figure above stops one step short of that, at the moment the plugin starts `swara play`. The last row closes most of the gap. It times the same `swara play` from its start to the first note it sends, at about 2.2 ms, so a first play reaches its first note in about 1,060 ms and a later one in about 4 ms. The first play's second goes almost entirely to FluidSynth starting, loading the MuseScore soundfont and opening its audio output, which happens once a session. The plugin starts that first `swara play` only once FluidSynth answers on its input, which FluidSynth reads only after its audio output is open, so the first note never reaches a player that cannot yet sound it. FluidSynth's own audio buffer, left at its defaults, adds a further delay between that note and the sound, and that delay was not measured. It would have to exceed 196 ms to miss the later-play target.

## How each figure is taken

### The server

Ready is the time from starting the server process to its reply to `initialize`, together with the `initialized` notification the client writes straight after. The command data LilyPond provides is in the cache before the timed runs start, as it is on every start after the first, although the server answers `initialize` without waiting for that data in any case.

Instant checks time a one-character edit on the song to the publish of diagnostics for that edit. The edit types a closing brace over the first note, and the next edit types the note back. The brace makes the parse report an error and the note clears it, so each publish shows by its errors which text it checked. The LilyPond check is switched off for this test, so that none of its publishes can be taken for an instant one.

The LilyPond check times the last edit to the publish that carries LilyPond's own messages for the edited text, the 330 ms pause included. The song carries one added line, `#(ly:warning "speed 0")`, and each edit changes its digit. An edit moves the earlier check's messages along with the text rather than clearing them, so only the publish that names the new digit can come from the new check.

Tidy times a `textDocument/formatting` request on the song. It separately times a fresh `swara format` process that reads the song on standard input and writes the result, with the start and exit of the process included.

Completion times a request just after a backslash typed on its own line at the top of the melody. The reply offers 728 items, and the time includes the client parsing all of them from JSON.

PDF after save times a save to the server's `swara/buildFinished` report that the PDF is in place beside the file. The 4-bar lesson is the song cut to its first four bars and four lyric lines, which is byte for byte what `cargo xtask gen-song --bars 4` writes. LilyPond's engraving sets the floor for these two, so their targets are approximate and the test fails only when a median exceeds its target by half again.

### The plugin

Colours time a one-character edit to the song, made with `nvim_buf_set_text`, until Tree-sitter has reparsed the buffer, which is the tree the highlighter reads. Each of 20 edits adds a space at the start of a different line in the middle of the song. Drawing the colours on screen happens at the next redraw and is not part of the figure, as the design's method states.

Playback runs the real FluidSynth with MuseScore 4's soundfont `MS Basic.sf3` and the `swara` that `cargo build` leaves in `target/debug`, on a scale engraved to MIDI by LilyPond. The test wraps Neovim's `vim.uv.spawn` to note the moment the plugin starts `swara play`, and times from the call to play. The first play includes starting FluidSynth and waiting until it answers on its input, and the second, about 1.5 s later, reuses the running player. Between the two, the test asks `lsof` for the network sockets of the FluidSynth the plugin started, and fails if it holds any.

The start-to-first-note row was taken separately, outside the test suite, by a script that reads the output of `swara play` in place of FluidSynth. It times 20 starts of the same `swara play` from the process start to the first `noteon` line on its output, after one warm-up start. A release build measured 2.7 ms on the same check, no faster, because the program does little work before its first note and most of the time is the operating system starting a process.

## Reproducing

The server's timing tests compile only into a release build and are marked ignored, so the ordinary test run skips them. They run one at a time, because a build running beside a timing would measure the build, and the whole set takes about 46 s.

```bash
cargo test -p swara --release --test speed -- --include-ignored --nocapture
```

The colour test runs with the rest of the plugin's tests and prints its median.

```bash
nvim --clean --headless -l tests/run.lua treesitter
```

The playback test runs only when asked, because it plays a scale out loud. It needs `cargo build` first, FluidSynth on the path and MuseScore 4 installed.

```bash
SWARA_REAL=1 nvim --clean --headless -l tests/run.lua playback
```
