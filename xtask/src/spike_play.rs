//! The playback spike: stream a MIDI file to a running FluidSynth as shell commands over TCP, each
//! at its own moment, and measure how late each command leaves.

use crate::smf::{self, Event, TimedEvent};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

/// The stop sequence. `reset` alone lets every voice fade through its release and the reverb, which
/// measured 580 ms to silence on a held note; zero gain first makes the cut immediate. The next
/// playback has to restore the gain.
const STOP: &[u8] = b"gain 0\nreset\n";

pub struct Report {
    /// How long after the command started the first note-on was written.
    pub first_noteon: Option<Duration>,
    /// How late each command was written, in the order sent.
    pub lateness: Vec<Duration>,
    /// When the stop was written, counted from the first note-on.
    pub stop_at: Option<Duration>,
}

/// Streams `events` to the shell at `addr`, timed from the moment the connection opens. With
/// `stop_after`, the stream stops at the first event due at or after that offset and sends the stop
/// sequence in its place.
pub fn stream(
    events: &[TimedEvent],
    addr: SocketAddr,
    stop_after: Option<Duration>,
    started: Instant,
) -> io::Result<Report> {
    let mut conn = TcpStream::connect(addr)?;
    // Small writes must leave at once; Nagle's algorithm would hold them back for an acknowledgement.
    conn.set_nodelay(true)?;
    // The shell answers every command with a prompt. Nothing reads those answers, so drain them, or the
    // socket buffers fill and the shell stops reading.
    let mut drain = conn.try_clone()?;
    thread::spawn(move || {
        let mut sink = [0u8; 4096];
        while matches!(drain.read(&mut sink), Ok(n) if n > 0) {}
    });

    let origin = Instant::now();
    let mut report = Report {
        first_noteon: None,
        lateness: Vec::with_capacity(events.len()),
        stop_at: None,
    };
    let mut first_sent = None;
    for e in events {
        let due = origin + e.at;
        wait_until(due);
        if stop_after.is_some_and(|r| e.at >= r) {
            conn.write_all(STOP)?;
            report.stop_at = first_sent.map(|first| Instant::now() - first);
            break;
        }
        conn.write_all(format!("{}\n", smf::shell_line(&e.event)).as_bytes())?;
        let sent = Instant::now();
        report.lateness.push(sent - due);
        if matches!(e.event, Event::NoteOn { .. }) && first_sent.is_none() {
            first_sent = Some(sent);
            report.first_noteon = Some(sent - started);
        }
    }
    conn.shutdown(std::net::Shutdown::Write)?;
    Ok(report)
}

/// Waits for `due` without spinning for long. On macOS a sleep overshoots by about a quarter of its
/// length, up to about 5 ms, so a long sleep stops 6 ms short, shorter sleeps of half the time left
/// close the gap, and only the last millisecond is spun.
fn wait_until(due: Instant) {
    loop {
        let now = Instant::now();
        if now >= due {
            return;
        }
        let left = due - now;
        if left > Duration::from_millis(8) {
            thread::sleep(left - Duration::from_millis(6));
        } else if left > Duration::from_millis(1) {
            thread::sleep(left / 2);
        } else {
            std::hint::spin_loop();
        }
    }
}

/// The nearest-rank percentile of an ascending slice; zero for an empty one.
pub fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let rank = (p * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// `cargo xtask spike-play`: reads the file, streams it to the shell on `port` and prints the measurements.
pub fn run(port: u16, path: &Path, stop_after: Option<Duration>) -> Result<(), String> {
    let started = Instant::now();
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let events = smf::read_smf(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let first = events
        .iter()
        .find(|e| matches!(e.event, Event::NoteOn { .. }))
        .map_or(Duration::ZERO, |e| e.at);
    let length = events.last().map_or(Duration::ZERO, |e| e.at);
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let report = stream(&events, addr, stop_after, started).map_err(|e| format!("{addr}: {e}"))?;

    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    println!(
        "file: {} events, first noteon at {:.3} s, last event at {:.3} s",
        events.len(),
        first.as_secs_f64(),
        length.as_secs_f64()
    );
    match report.first_noteon {
        Some(d) => println!(
            "first noteon written {:.1} ms after the command started",
            ms(d)
        ),
        None => println!("no noteon was written"),
    }
    let mut late = report.lateness.clone();
    late.sort();
    println!(
        "lateness over {} commands: p50 {:.3} ms, p99 {:.3} ms, max {:.3} ms",
        late.len(),
        ms(percentile(&late, 0.50)),
        ms(percentile(&late, 0.99)),
        ms(percentile(&late, 1.0)),
    );
    if let Some(d) = report.stop_at {
        println!("stop written {:.1} ms after the first noteon", ms(d));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smf::{Event, TimedEvent};
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    fn at(ms: u64, event: Event) -> TimedEvent {
        TimedEvent {
            at: Duration::from_millis(ms),
            event,
        }
    }

    /// Accepts one connection and returns each line it receives with the time it arrived.
    fn listen() -> (
        std::net::SocketAddr,
        std::thread::JoinHandle<Vec<(String, Instant)>>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            BufReader::new(conn)
                .lines()
                .map(|l| (l.unwrap(), Instant::now()))
                .collect()
        });
        (addr, handle)
    }

    #[test]
    fn streams_shell_lines_in_order_and_on_time() {
        let (addr, handle) = listen();
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
        ];
        let report = stream(&events, addr, None, Instant::now()).unwrap();
        let got = handle.join().unwrap();
        let lines: Vec<&str> = got.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(lines, ["prog 0 24", "noteon 0 60 100", "noteoff 0 60"]);
        let gap = got[2].1 - got[1].1;
        assert!(
            gap >= Duration::from_millis(55) && gap < Duration::from_millis(100),
            "gap {gap:?}"
        );
        assert_eq!(report.lateness.len(), 3);
        assert!(report.first_noteon.is_some());
        assert_eq!(report.stop_at, None);
    }

    #[test]
    fn stop_after_ends_the_stream_with_gain_zero_and_reset() {
        let (addr, handle) = listen();
        let events = [
            at(
                0,
                Event::NoteOn {
                    ch: 0,
                    key: 60,
                    vel: 100,
                },
            ),
            at(
                40,
                Event::NoteOn {
                    ch: 0,
                    key: 62,
                    vel: 100,
                },
            ),
            at(80, Event::NoteOff { ch: 0, key: 60 }),
        ];
        let report = stream(
            &events,
            addr,
            Some(Duration::from_millis(50)),
            Instant::now(),
        )
        .unwrap();
        let lines: Vec<String> = handle.join().unwrap().into_iter().map(|(l, _)| l).collect();
        assert_eq!(
            lines,
            ["noteon 0 60 100", "noteon 0 62 100", "gain 0", "reset"]
        );
        let stop_at = report.stop_at.unwrap();
        assert!(
            stop_at >= Duration::from_millis(48) && stop_at < Duration::from_millis(90),
            "stop {stop_at:?}"
        );
    }

    /// A sleep on macOS has measured up to 4 ms late, so a command that waits must still leave on time.
    /// The median holds the 1 ms bound, which a plain sleep misses on every command; the worst case
    /// gets 5 ms, because one descheduling on a shared machine is not a fault in the wait.
    #[test]
    fn commands_after_a_wait_leave_within_a_millisecond() {
        let (addr, handle) = listen();
        let events: Vec<TimedEvent> = (0..10)
            .map(|i| at(i * 25, Event::NoteOff { ch: 0, key: 60 }))
            .collect();
        let report = stream(&events, addr, None, Instant::now()).unwrap();
        handle.join().unwrap();
        let mut late = report.lateness.clone();
        late.sort();
        let median = percentile(&late, 0.50);
        let worst = *late.last().unwrap();
        assert!(median < Duration::from_millis(1), "median {median:?}");
        assert!(worst < Duration::from_millis(5), "worst {worst:?}");
    }

    #[test]
    fn percentile_takes_the_nearest_rank() {
        let ms: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(percentile(&ms, 0.50), Duration::from_millis(50));
        assert_eq!(percentile(&ms, 0.99), Duration::from_millis(99));
        assert_eq!(percentile(&ms, 1.00), Duration::from_millis(100));
        assert_eq!(percentile(&[], 0.99), Duration::ZERO);
    }
}
