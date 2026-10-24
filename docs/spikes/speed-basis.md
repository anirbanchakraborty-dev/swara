# Speed basis

Every speed target in Swara is measured against one fixed file, `bench/song-100.ly`, so that numbers taken weeks apart describe the same work. This note records what that file is and what it cost to engrave when it was first generated.

## The song

`cargo xtask gen-song --bars 100` writes the file. It is a guitar lesson in miniature: a melody of 800 eighth notes in 4/4 and C major on a `treble_8` staff, the same melody as tablature in standard tuning, two note-name lines, and one lyric syllable per note attached with `\lyricsto`. The score asks for both a PDF (`\layout`) and a MIDI file (`\midi`). The melody cycles a fixed sixteen-note pattern between G3 and C5, so the output depends only on the bar count and regenerating the file changes nothing.

## The measurement

Measured on 2026-10-24 with LilyPond 2.26.0 (Guile 3.0) on an Apple M5 Pro laptop running macOS 27.0.1, using hyperfine 2.0.0 with one warm-up run and five timed runs. Times are wall clock, in milliseconds.

| File and run | Median | Min | Max |
| --- | --- | --- | --- |
| `song-100.ly`, timed alone | 1,311 | 1,291 | 1,328 |
| `song-100.ly`, paired run | 1,349 | 1,343 | 1,413 |
| earlier hand-written song, paired run | 1,359 | 1,320 | 1,374 |

The design's figure of 1,493 ms came from an earlier hand-written song of the same shape, measured before this generator existed and not kept in the repository; it differs from the generated song mainly in using plain `\lyricmode` lyrics rather than `\lyricsto`. The plan asked for the generated song to land within 25% of that figure, and it lands 12% under it. The paired run is the stronger evidence: timed back to back in one hyperfine session, the two songs differ by 10 ms, which is inside one standard deviation of either (25 ms and 29 ms). The gap to 1,493 ms therefore reflects the machine's state on a different day, not a lighter file.

The command was:

```bash
hyperfine -N -w 1 -r 5 'env LC_ALL=C lilypond -l WARNING -o <scratch>/s bench/song-100.ly'
```

`-N` runs the command without a shell, which keeps a few milliseconds of shell start-up out of each run. `LC_ALL=C` keeps LilyPond's messages in English, so that later tests can match them, and `-l WARNING` prints only warnings and errors.

## For later tasks

Every later speed test compiles this file rather than a fresh one, and the 1,311 ms median above is the figure to quote a LilyPond build against. One detail matters to any script that reads hyperfine's output: hyperfine 2.0 moved the wall-clock statistics in its JSON export to `results[].summary.time_wall_clock`, so a script written for the 1.x layout, which reads `results[].median`, fails on a missing key.
