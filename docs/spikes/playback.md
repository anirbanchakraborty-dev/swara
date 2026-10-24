# Warm playback spike

Quick playback should start sounding the moment it is asked for. FluidSynth 2.6.1 takes about a second to start and load a soundfont, so a fresh process per playback spends that second before the first note. Its shell has no command that loads a MIDI file into a running player, but it does take live events (`noteon`, `noteoff`, `cc`, `prog`, `pitch_bend`, `reset`) over TCP. This spike asked whether a player kept running in the background can be fed a file's events one at a time, on time, and stopped cleanly. It can. Everything here was measured on 2026-10-24 with FluidSynth 2.6.1 from Homebrew and the MuseScore 4 soundfont `MS Basic.sf3`, on the machine described in `speed-basis.md`, using the 100-bar test song built to MIDI. LilyPond's default MIDI tempo is a quarter note at 60, so that file lasts 400 s and holds 3,202 events.

## The verdict

Stream. A player started once and kept warm takes each playback's first note 0.8 ms after the request, against about 1.06 s for a fresh process, and the stream keeps time to a tenth of a millisecond over the whole song. Every figure the plan asked for is met with a wide margin.

| Check | Target | Measured |
| --- | --- | --- |
| First `noteon` written after the command starts | within 0.2 s | 0.8 ms |
| Lateness, p99 over all 3,202 commands | under 5 ms | 0.121 ms (max 0.572 ms) |
| Non-silent length of the rendered sound against the MIDI length | within 2% either way | +0.18% |
| Silence after a stop | within 50 ms | at once, within the 2 ms resolution |

`smf.rs` reads Standard MIDI Files of formats 0 and 1 into timed events, and `shell_line` turns each event into the shell command that plays it. The server plan moves both into the server crate.

## Starting the player

The player is launched as `fluidsynth -s -i -o shell.port=<p> -o shell.prompt=<token> -o synth.dynamic-sample-loading=1 <soundfont>`. Over five launches it was ready, meaning that it answered on its port, after a median of 1,056 ms (1,048 ms to 1,074 ms). The two players launched for the long runs were ready after 1,063 ms and 1,059 ms. Dynamic sample loading is what makes this fast: it loads each instrument's samples when a note first needs them, and without it the port took more than 4 s to open. The server therefore pays this second once per session, when the first playback is asked for, and never again.

## Finding a free port

The plan's strategy was to try ports upward from `9800 + (pid % 200)` and to treat a refused connection after 2 s, or a bind error on FluidSynth's stderr, as a busy port. Neither test is enough on macOS, because of what FluidSynth does when its port is taken.

- When another program, here `nc -l`, holds the port on IPv4, FluidSynth binds the IPv6 side of the same port without complaint, stays running and prints nothing. A connection to `127.0.0.1` then reaches the other program, so the connection is not refused and stderr is silent.
- When another FluidSynth holds the port, the new one prints `Got error 48 while trying to bind server socket`, then `Failed to create the server.` and `Continuing without it.`, and exits with status 1. The bind-error rule catches this one, but a first attempt that checked identity by sending `echo` was answered by the other FluidSynth and called the port ready.

What works is a token. Each player is launched with `shell.prompt` set to a string unique to the process and the port, `swara-<pid>-<port>>`, and FluidSynth sends its prompt the moment a client connects. A port is the player's own only when the first bytes it sends back are that token. Another program sends something else or nothing, and another FluidSynth sends its own prompt. With the first port held by `nc`, the harness reported it busy and started on the next port, ready after 1,112 ms. Held by another FluidSynth, it did the same, ready after 1,148 ms. A port where the process exits, prints `bind`, or has not answered within 2 s also counts as busy, and the player launched for it is killed before the next port is tried.

## Streaming the song

`cargo xtask spike-play --port <p> <file.midi>` reads the file, opens one TCP connection with Nagle's algorithm off, so that each small write leaves at once, and writes each event's shell line at its own moment, timed from when the connection opened. A second thread reads and discards what the shell sends back, because it answers every command with its prompt and an unread socket would eventually stop it reading. Lateness is how long after its due moment each command was written.

The first full run was late by about 4 ms on every command that followed a wait, while commands due at the same moment as the one before them left within 0.1 ms. The cause was the operating system's sleep, which on this Mac overshoots by about a quarter of the time asked for, up to about 5 ms:

| Sleep asked for | Overshoot, median | Overshoot, max |
| --- | --- | --- |
| 1 ms | 0.259 ms | 0.278 ms |
| 5 ms | 1.262 ms | 1.282 ms |
| 25 ms | 4.218 ms | 5.021 ms |
| 499 ms | 5.019 ms | 5.029 ms |

The wait now sleeps until 6 ms before the due moment, closes the rest with sleeps of half the time left, and spins only for the last millisecond. The test `commands_after_a_wait_leave_within_a_millisecond` failed at 4.05 ms before this change and passes after it. Over the whole song the stream's lateness was then p50 0.053 ms, p99 0.121 ms and max 0.572 ms. The live player ran with its gain at zero (`-g 0`), so that the speakers stayed quiet. Lateness is measured where the commands leave, so the gain does not affect it.

## Checking the sound offline

A second player wrote its sound to a WAV file instead of the speakers (`-a file -o audio.file.name=<tmp>/offline.wav`), and the song was streamed into it. FluidSynth's file driver renders in real time rather than as fast as it can, so the file is a faithful recording of what a listener would hear. The non-silent length of the file, from the first to the last sample above about −60 dBFS, was 400.728 s against the MIDI's 400.000 s, which is 0.18% longer. The extra is presumably the release of the last notes, which the measurement did not separate out. The stream into this player had its own lateness of p50 0.044 ms, p99 0.086 ms and max 0.116 ms. The two 400 s runs ran at the same time to halve the wait, which can only have made both lateness figures worse.

Two details of the file driver matter to anyone repeating this. The file starts when the player starts, not at the first note, so lengths are measured from the first audible sample. And a player stopped with a signal never finishes the WAV header, which leaves the data length at zero, so the reader takes the data to the end of the file.

## Stopping

The plan expected `reset` to silence the player within 50 ms. It does not. `reset` ends every note, but each one then fades through its release and the reverb's tail. To find a stop that is immediate, six candidates were sent one second into a held organ note (program 19 counted from zero, General MIDI's church organ, which holds its level until it is released) at −29.4 dBFS, and the recorded sound was examined in 10 ms steps.

| Stop sent | 20 dB down after | Below −60 dBFS after |
| --- | --- | --- |
| `reset` | 220 ms | 580 ms |
| `gain 0`, then `reset` | 0 ms | 0 ms |
| `cc <ch> 7 0` on all 16 channels, then `reset` | 0 ms | 0 ms |
| `set synth.reverb.active 0`, then `reset` | 150 ms | 230 ms |
| `set synth.reverb.active 0`, then `cc <ch> 120 0` on all 16 channels | 0 ms | 0 ms |
| `cc <ch> 120 0` on all 16 channels | 270 ms | 830 ms |

On the song itself, `reset` alone had left sound 122 ms after it. The stop is now `gain 0` followed by `reset`, two short lines instead of seventeen. `spike-play --stop-after <ms>` sends it in place of the first event due at or after that offset. Stopped at 2.0 s, 5.0 s and 9.5 s into the song, the last sample above −60 dBFS fell 0.2 ms, 2.0 ms and 1.7 ms before the computed moment of the stop. That moment is dated from the first audible sample, and a note's attack takes a millisecond or two to rise above the threshold, so a figure this small means silent at once.

Zero gain stays set after the stop, so the next playback has to send `gain <value>` before its first note. Sending `gain 0.2`, FluidSynth's default, after a stop brought a held note back to between −29 and −31 dBFS.

## Facts the server has to honour

- FluidSynth's shell sends nothing on connect unless `shell.prompt` is set. With it set, it sends the prompt on connect and after every command. Whatever it sends has to be read and discarded.
- The shell's `gain` command accepts 0 although its help text gives the range as greater than 0 and less than 5. `get synth.gain` does not reflect a gain set this way, so the server has to remember the gain it set rather than ask for it.
- The shell has no command for the reverb or the chorus as a whole, but `set synth.reverb.active 0` works.
- FluidSynth listens on the IPv6 wildcard address of its port, which also accepts IPv4 connections, unless another program already holds the IPv4 side.
- The shell's help lists `quit`, described as quitting the synthesizer. The spike did not test it and stopped every player with a signal instead.

## Reproducing

`spikes/playback/run.py` launches the players, finds the ports, runs `cargo xtask spike-play` and measures the WAV files. It is run from the repository root with `python3 -I`, and writes its scratch files to `$TMPDIR/swara-playback-spike`.

```bash
python3 -I spikes/playback/run.py cold 5
python3 -I spikes/playback/run.py busy nc
python3 -I spikes/playback/run.py busy fluidsynth
python3 -I spikes/playback/run.py stream <song.midi>
python3 -I spikes/playback/run.py offline <song.midi>
python3 -I spikes/playback/run.py stop <song.midi> 5000
python3 -I spikes/playback/run.py stops
```

The song's MIDI file comes from building `bench/song-100.ly` with LilyPond, and it is not kept in the repository.
