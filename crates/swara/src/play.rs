//! `swara play`: writes a MIDI file to its standard output as the commands FluidSynth's shell
//! takes, each at its own moment. The plugin points that output at the standard input of a
//! FluidSynth it keeps running, so a player kept warm sounds the first note within a millisecond,
//! where a fresh one spends about a second loading its soundfont, as `docs/spikes/playback.md`
//! measured. Stopping is the plugin's job: it ends this process, then sends `gain 0` and `reset`
//! itself.

mod smf;

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use smf::TimedEvent;

/// FluidSynth's own default gain. Each playback sets it first, because the plugin's stop leaves it
/// at zero.
const DEFAULT_GAIN: f64 = 0.2;
/// The highest gain FluidSynth accepts.
const MAX_GAIN: f64 = 10.0;

/// Why a playback ended early.
#[derive(Debug)]
pub enum PlayError {
    /// The file could not be read, or is not a MIDI file this can play.
    File(String),
    /// Writing to FluidSynth failed.
    Io(io::Error),
}

impl fmt::Display for PlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlayError::File(message) => f.write_str(message),
            PlayError::Io(error) if error.kind() == io::ErrorKind::BrokenPipe => {
                f.write_str("FluidSynth has stopped reading")
            }
            PlayError::Io(error) => write!(f, "{error}"),
        }
    }
}

/// `swara play`, given the arguments after `play`.
pub fn main(args: &[&str]) -> ExitCode {
    let Some((gain, file)) = parse(args) else {
        eprintln!("{}", crate::USAGE);
        return ExitCode::from(2);
    };
    match play(gain, Path::new(file), &mut io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("swara play: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The gain and the file, in either order.
fn parse<'a>(args: &[&'a str]) -> Option<(f64, &'a str)> {
    let (mut gain, mut file) = (DEFAULT_GAIN, None);
    let mut args = args.iter();
    while let Some(&arg) = args.next() {
        match arg {
            "--gain" => {
                let valid = |gain: &f64| (0.0..=MAX_GAIN).contains(gain);
                gain = args.next()?.parse().ok().filter(valid)?;
            }
            _ if file.is_none() && !arg.starts_with("--") => file = Some(arg),
            _ => return None,
        }
    }
    Some((gain, file?))
}

/// Reads `file`, then streams it to `out` at `gain`. A file it cannot play fails before anything
/// is written.
pub fn play(gain: f64, file: &Path, out: &mut impl Write) -> Result<(), PlayError> {
    let unreadable =
        |error: &dyn fmt::Display| PlayError::File(format!("{}: {error}", file.display()));
    let bytes = fs::read(file).map_err(|e| unreadable(&e))?;
    let events = smf::read_smf(&bytes).map_err(|e| unreadable(&e))?;
    stream(&events, out, gain).map_err(PlayError::Io)
}

/// Streams `events` to `out`: the gain first, then each event at its moment counted from the start,
/// then `reset`. Each command goes out in a single write of one whole line, because the plugin
/// writes its own commands to the same pipe, and a write that small lands in one piece.
pub fn stream(events: &[TimedEvent], out: &mut impl Write, gain: f64) -> io::Result<()> {
    let mut send = |line: String| -> io::Result<()> {
        out.write_all(line.as_bytes())?;
        out.flush()
    };
    let origin = Instant::now();
    send(format!("gain {gain}\n"))?;
    for event in events {
        wait_until(origin + event.at);
        send(format!("{}\n", smf::shell_line(&event.event)))?;
    }
    send("reset\n".to_owned())
}

/// Waits for `due` without spinning for long. On macOS a sleep can end up to half its length late,
/// so each sleep is half the time left, which ends before `due` even then, and only the last
/// millisecond is spun.
fn wait_until(due: Instant) {
    loop {
        let now = Instant::now();
        if now >= due {
            return;
        }
        let left = due - now;
        if left > Duration::from_millis(1) {
            thread::sleep(left / 2);
        } else {
            std::hint::spin_loop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::smf::Event;
    use super::*;
    use crate::testing::Scratch;

    /// A format-0 file at 480 ticks a quarter note: middle C for one quarter note.
    const MIDDLE_C: &[u8] = &[
        b'M', b'T', b'h', b'd', 0, 0, 0, 6, 0, 0, 0, 1, 0x01, 0xE0, //
        b'M', b'T', b'r', b'k', 0, 0, 0, 13, //
        0x00, 0x90, 60, 100, //
        0x83, 0x60, 0x80, 60, 0, //
        0x00, 0xFF, 0x2F, 0x00,
    ];

    fn at(ms: u64, event: Event) -> TimedEvent {
        TimedEvent {
            at: Duration::from_millis(ms),
            event,
        }
    }

    /// Stands in for the pipe to FluidSynth: keeps each write it is given, with the time it came.
    #[derive(Default)]
    struct Recorder(Vec<(String, Instant)>);

    impl Write for Recorder {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0
                .push((String::from_utf8(buf.to_vec()).unwrap(), Instant::now()));
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A pipe whose reader has gone, as when FluidSynth has exited.
    struct Gone;

    impl Write for Gone {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn streams_in_order_on_time() {
        let events = [
            at(0, Event::Program { ch: 0, num: 24 }),
            at(
                0,
                Event::NoteOn {
                    ch: 0,
                    key: 60,
                    vel: 100,
                },
            ),
            at(60, Event::NoteOff { ch: 0, key: 60 }),
            at(
                90,
                Event::NoteOn {
                    ch: 1,
                    key: 64,
                    vel: 80,
                },
            ),
        ];
        let mut out = Recorder::default();
        let start = Instant::now();
        stream(&events, &mut out, DEFAULT_GAIN).unwrap();
        let writes: Vec<&str> = out.0.iter().map(|(text, _)| text.as_str()).collect();
        let expected = [
            "gain 0.2\n",
            "prog 0 24\n",
            "noteon 0 60 100\n",
            "noteoff 0 60\n",
            "noteon 1 64 80\n",
            "reset\n",
        ];
        assert_eq!(writes, expected);
        for (event, (line, written)) in events.iter().zip(&out.0[1..]) {
            let after = *written - start;
            assert!(
                after >= event.at,
                "{line:?} written after {after:?}, due at {:?}",
                event.at
            );
            let bound = event.at + Duration::from_millis(200);
            assert!(
                after < bound,
                "{line:?} written after {after:?}, due at {:?}",
                event.at
            );
        }

        let mut out = Recorder::default();
        stream(&events[..1], &mut out, 0.5).unwrap();
        assert_eq!(out.0[0].0, "gain 0.5\n");
    }

    #[test]
    fn a_file_it_cannot_read_sends_nothing() {
        let scratch = Scratch::new("play-unreadable");
        let mut out = Recorder::default();
        let missing = scratch.path().join("missing.midi");
        let error = play(DEFAULT_GAIN, &missing, &mut out).unwrap_err();
        assert!(matches!(error, PlayError::File(_)), "{error}");
        let not_midi = scratch.path().join("song.midi");
        fs::write(&not_midi, b"not a MIDI file").unwrap();
        assert!(matches!(
            play(DEFAULT_GAIN, &not_midi, &mut out),
            Err(PlayError::File(_))
        ));
        assert!(out.0.is_empty(), "{:?}", out.0);
    }

    #[test]
    fn plays_a_file() {
        let scratch = Scratch::new("play-file");
        let file = scratch.path().join("song.midi");
        fs::write(&file, MIDDLE_C).unwrap();
        let mut out = Recorder::default();
        play(DEFAULT_GAIN, &file, &mut out).unwrap();
        let writes: Vec<&str> = out.0.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(writes[0], "gain 0.2\n");
        assert_eq!(writes[1], "noteon 0 60 100\n");
        assert_eq!(writes[writes.len() - 1], "reset\n");
    }

    #[test]
    fn a_player_that_has_gone_is_named() {
        let scratch = Scratch::new("play-gone");
        let file = scratch.path().join("song.midi");
        fs::write(&file, MIDDLE_C).unwrap();
        let error = play(DEFAULT_GAIN, &file, &mut Gone).unwrap_err();
        assert_eq!(error.to_string(), "FluidSynth has stopped reading");
    }

    #[test]
    fn flags_come_in_any_order() {
        assert_eq!(parse(&["a.midi"]), Some((DEFAULT_GAIN, "a.midi")));
        assert_eq!(parse(&["--gain", "0.5", "a.midi"]), Some((0.5, "a.midi")));
        assert_eq!(parse(&["a.midi", "--gain", "0.5"]), Some((0.5, "a.midi")));
        for bad in [
            &[][..],
            &["--gain", "0.5"],
            &["--gain"],
            &["--port", "9800", "a.midi"],
            &["--gain", "11", "a.midi"],
            &["--gain", "NaN", "a.midi"],
            &["a.midi", "b.midi"],
            &["--loud", "a.midi"],
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
    }

    /// A sleep on macOS can end late, so a wait must end on time anyway. The median holds the 1 ms
    /// bound, which a plain sleep misses every time. The worst case gets 5 ms, because one
    /// descheduling on a shared machine is not a fault in the wait.
    #[test]
    fn waits_end_within_a_millisecond() {
        let mut late: Vec<Duration> = (0..10)
            .map(|_| {
                let due = Instant::now() + Duration::from_millis(25);
                wait_until(due);
                Instant::now() - due
            })
            .collect();
        late.sort();
        assert!(
            late[(late.len() - 1) / 2] < Duration::from_millis(1),
            "{late:?}"
        );
        assert!(late[late.len() - 1] < Duration::from_millis(5), "{late:?}");
    }

    /// A sleep on macOS can end up to half its length late, which a long first sleep turns into a
    /// late wait. Measured on 2026-10-25: the overshoot reached 0.50 of the sleep at every length
    /// from 1 ms to 19 ms.
    #[test]
    fn long_waits_end_within_a_millisecond() {
        let mut late: Vec<Duration> = (0..5)
            .map(|_| {
                let due = Instant::now() + Duration::from_millis(100);
                wait_until(due);
                Instant::now() - due
            })
            .collect();
        late.sort();
        assert!(late[late.len() / 2] < Duration::from_millis(1), "{late:?}");
        assert!(late[late.len() - 1] < Duration::from_millis(5), "{late:?}");
    }
}
