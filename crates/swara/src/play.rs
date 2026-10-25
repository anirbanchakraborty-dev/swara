//! `swara play`: streams a MIDI file to a FluidSynth that is already running, as the commands its
//! shell takes over TCP, each at its own moment. A player kept warm sounds the first note within a
//! millisecond, where a fresh one spends about a second loading its soundfont, as
//! `docs/spikes/playback.md` measured. Stopping is the plugin's job: it ends this process, then
//! sends `gain 0` and `reset` itself.

mod smf;

use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::path::Path;
use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use smf::TimedEvent;

/// FluidSynth's own default gain. Each playback sets it first, because the plugin's stop leaves it
/// at zero.
const DEFAULT_GAIN: f64 = 0.2;
/// The highest gain FluidSynth accepts.
const MAX_GAIN: f64 = 10.0;
/// How long to wait, once `reset` is written, for the shell to close its side.
const CLOSE_WAIT: Duration = Duration::from_secs(1);

/// Why a playback ended early.
#[derive(Debug)]
pub enum PlayError {
    /// Nothing listens on the port, so no player is running there.
    Refused(SocketAddr),
    /// The file could not be read, or is not a MIDI file this can play.
    File(String),
    /// The connection failed after it opened.
    Io(io::Error),
}

impl PlayError {
    /// The exit status: 3 when no player is running, so the plugin knows to start one.
    fn code(&self) -> u8 {
        match self {
            PlayError::Refused(_) => 3,
            PlayError::File(_) | PlayError::Io(_) => 1,
        }
    }
}

impl fmt::Display for PlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlayError::Refused(addr) => write!(f, "no player is listening on {addr}"),
            PlayError::File(message) => f.write_str(message),
            PlayError::Io(error) => write!(f, "{error}"),
        }
    }
}

/// `swara play`, given the arguments after `play`.
pub fn main(args: &[&str]) -> ExitCode {
    let Some((port, gain, file)) = parse(args) else {
        eprintln!("{}", crate::USAGE);
        return ExitCode::from(2);
    };
    match play(port, gain, Path::new(file)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("swara play: {error}");
            ExitCode::from(error.code())
        }
    }
}

/// The port, the gain and the file, with the flags in any order.
fn parse<'a>(args: &[&'a str]) -> Option<(u16, f64, &'a str)> {
    let (mut port, mut gain, mut file) = (None, DEFAULT_GAIN, None);
    let mut args = args.iter();
    while let Some(&arg) = args.next() {
        match arg {
            "--port" => port = Some(args.next()?.parse().ok().filter(|&port: &u16| port != 0)?),
            "--gain" => {
                let valid = |gain: &f64| (0.0..=MAX_GAIN).contains(gain);
                gain = args.next()?.parse().ok().filter(valid)?;
            }
            _ if file.is_none() && !arg.starts_with("--") => file = Some(arg),
            _ => return None,
        }
    }
    Some((port?, gain, file?))
}

/// Reads `file`, then streams it to the player on `port` at `gain`. A file it cannot play fails
/// before the player is touched.
pub fn play(port: u16, gain: f64, file: &Path) -> Result<(), PlayError> {
    let unreadable =
        |error: &dyn fmt::Display| PlayError::File(format!("{}: {error}", file.display()));
    let bytes = fs::read(file).map_err(|e| unreadable(&e))?;
    let events = smf::read_smf(&bytes).map_err(|e| unreadable(&e))?;
    stream(&events, SocketAddr::from((Ipv4Addr::LOCALHOST, port)), gain)
}

/// Streams `events` to the shell at `addr`: the gain first, then each event at its moment counted
/// from when the connection opened, then `reset`. Returns once the shell has closed its side, or
/// after `CLOSE_WAIT`.
pub fn stream(events: &[TimedEvent], addr: SocketAddr, gain: f64) -> Result<(), PlayError> {
    let mut conn = TcpStream::connect(addr).map_err(|error| match error.kind() {
        io::ErrorKind::ConnectionRefused => PlayError::Refused(addr),
        _ => PlayError::Io(error),
    })?;
    // Small writes must leave at once. Nagle's algorithm would hold each back for an acknowledgement.
    conn.set_nodelay(true).map_err(PlayError::Io)?;
    let closed = drain(&conn).map_err(PlayError::Io)?;
    let origin = Instant::now();
    let mut write = || -> io::Result<()> {
        conn.write_all(format!("gain {gain}\n").as_bytes())?;
        for event in events {
            wait_until(origin + event.at);
            conn.write_all(format!("{}\n", smf::shell_line(&event.event)).as_bytes())?;
        }
        conn.write_all(b"reset\n")?;
        conn.shutdown(Shutdown::Write)
    };
    write().map_err(PlayError::Io)?;
    // Closing a socket with data still unread can reset the connection, and a reset can drop
    // commands the shell has not read yet, `reset` among them. Once the shell has read to the end
    // it closes its side, and nothing is left unread.
    let _ = closed.recv_timeout(CLOSE_WAIT);
    Ok(())
}

/// Reads and discards what the shell sends, a prompt for every command, for as long as the
/// connection is open. An unread socket fills, and the shell then stops reading. The receiver
/// hears when the shell has closed its side.
fn drain(conn: &TcpStream) -> io::Result<mpsc::Receiver<()>> {
    let mut answers = conn.try_clone()?;
    let (closed, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut sink = [0; 4096];
        while matches!(answers.read(&mut sink), Ok(read) if read > 0) {}
        let _ = closed.send(());
    });
    Ok(receiver)
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
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::thread::JoinHandle;

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

    /// Accepts one connection, writes `answer` back after each line it receives, as the shell
    /// answers each command with its prompt, and returns each line with the time it arrived.
    fn listen(answer: Vec<u8>) -> (SocketAddr, JoinHandle<Vec<(String, Instant)>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            let mut answers = conn.try_clone().unwrap();
            let mut lines = Vec::new();
            for line in BufReader::new(conn).lines() {
                let Ok(line) = line else { break };
                lines.push((line, Instant::now()));
                if answers.write_all(&answer).is_err() {
                    break;
                }
            }
            lines
        });
        (addr, handle)
    }

    #[test]
    fn connection_refused_exits_3() {
        let scratch = Scratch::new("play-refused");
        let file = scratch.path().join("song.midi");
        fs::write(&file, MIDDLE_C).unwrap();
        // A port just let go of has nothing listening on it.
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let port = port.to_string();
        assert_eq!(
            main(&["--port", &port, file.to_str().unwrap()]),
            ExitCode::from(3)
        );
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
        let (addr, handle) = listen(Vec::new());
        let start = Instant::now();
        stream(&events, addr, DEFAULT_GAIN).unwrap();
        let got = handle.join().unwrap();
        let lines: Vec<&str> = got.iter().map(|(line, _)| line.as_str()).collect();
        let expected = [
            "gain 0.2",
            "prog 0 24",
            "noteon 0 60 100",
            "noteoff 0 60",
            "noteon 1 64 80",
            "reset",
        ];
        assert_eq!(lines, expected);
        for (event, (line, arrived)) in events.iter().zip(&got[1..]) {
            let after = *arrived - start;
            assert!(
                after >= event.at,
                "{line} arrived after {after:?}, due at {:?}",
                event.at
            );
            let bound = event.at + Duration::from_millis(200);
            assert!(
                after < bound,
                "{line} arrived after {after:?}, due at {:?}",
                event.at
            );
        }

        let (addr, handle) = listen(Vec::new());
        stream(&events[..1], addr, 0.5).unwrap();
        assert_eq!(handle.join().unwrap()[0].0, "gain 0.5");
    }

    /// A shell that answers each command with 64 KiB fills both socket buffers within a few
    /// commands unless its answers are read, and then stops reading.
    #[test]
    fn drains_what_the_shell_sends() {
        let events: Vec<TimedEvent> = (0..1024u16)
            .map(|i| {
                at(
                    0,
                    Event::NoteOff {
                        ch: 0,
                        key: (i % 128) as u8,
                    },
                )
            })
            .collect();
        let (addr, handle) = listen(vec![b'>'; 64 * 1024]);
        let (done, finished) = mpsc::channel();
        thread::spawn(move || {
            let _ = done.send(stream(&events, addr, DEFAULT_GAIN).is_ok());
        });
        let outcome = finished.recv_timeout(Duration::from_secs(10));
        assert_eq!(outcome, Ok(true), "the stream stalled or failed");
        let lines = handle.join().unwrap();
        assert_eq!(lines.len(), 1026, "the shell read {} lines", lines.len());
        assert_eq!(lines[1025].0, "reset");
    }

    #[test]
    fn flags_come_in_any_order() {
        assert_eq!(
            parse(&["--port", "9800", "a.midi"]),
            Some((9800, DEFAULT_GAIN, "a.midi"))
        );
        assert_eq!(
            parse(&["--gain", "0.5", "--port", "9800", "a.midi"]),
            Some((9800, 0.5, "a.midi"))
        );
        for bad in [
            &["a.midi"][..],
            &["--port", "9800"],
            &["--port", "0", "a.midi"],
            &["--port", "9800", "--gain", "11", "a.midi"],
            &["--port", "9800", "--gain", "NaN", "a.midi"],
            &["--port", "9800", "a.midi", "b.midi"],
            &["--port", "9800", "--loud", "a.midi"],
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
