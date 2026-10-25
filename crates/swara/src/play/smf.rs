//! A minimal Standard MIDI File reader, formats 0 and 1, for `swara play`. It keeps the channel
//! messages FluidSynth's shell can take as live events and turns ticks into wall-clock offsets.

use std::fmt;
use std::time::Duration;

/// A channel message in the form FluidSynth's shell accepts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    NoteOn { ch: u8, key: u8, vel: u8 },
    NoteOff { ch: u8, key: u8 },
    Control { ch: u8, ctrl: u8, value: u8 },
    Program { ch: u8, num: u8 },
    PitchBend { ch: u8, value: u16 },
}

/// An event and its offset from the start of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedEvent {
    pub at: Duration,
    pub event: Event,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmfError {
    NotMidi,
    Truncated,
    UnsupportedFormat(u16),
    UnsupportedDivision(u16),
    Malformed(&'static str),
}

impl fmt::Display for SmfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SmfError::NotMidi => write!(f, "not a Standard MIDI File"),
            SmfError::Truncated => write!(f, "the file ends inside a chunk"),
            SmfError::UnsupportedFormat(n) => {
                write!(f, "MIDI format {n} is not read, only formats 0 and 1")
            }
            SmfError::UnsupportedDivision(d) => {
                write!(f, "time division {d:#06x} is not ticks per quarter note")
            }
            SmfError::Malformed(what) => write!(f, "malformed track: {what}"),
        }
    }
}

impl std::error::Error for SmfError {}

/// The default tempo of a Standard MIDI File, in microseconds per quarter note (120 beats a minute).
const DEFAULT_TEMPO: u64 = 500_000;

enum Item {
    Tempo(u64),
    Channel(Event),
}

/// Reads a format 0 or format 1 file and returns its channel events in time order.
pub fn read_smf(bytes: &[u8]) -> Result<Vec<TimedEvent>, SmfError> {
    let mut rest = bytes;
    let (id, head) = take_chunk(&mut rest).map_err(|_| SmfError::NotMidi)?;
    if id != b"MThd" || head.len() < 6 {
        return Err(SmfError::NotMidi);
    }
    let format = u16::from_be_bytes([head[0], head[1]]);
    let division = u16::from_be_bytes([head[4], head[5]]);
    if format > 1 {
        return Err(SmfError::UnsupportedFormat(format));
    }
    if division & 0x8000 != 0 || division == 0 {
        return Err(SmfError::UnsupportedDivision(division));
    }

    // Every item carries its absolute tick; a stable sort on the tick keeps the conductor track's
    // tempo changes ahead of notes on the same tick and keeps each track's own order.
    let mut items: Vec<(u64, Item)> = Vec::new();
    while !rest.is_empty() {
        let (id, body) = take_chunk(&mut rest)?;
        if id == b"MTrk" {
            read_track(body, &mut items)?;
        }
    }
    items.sort_by_key(|(tick, _)| *tick);

    // Time is accumulated in microsecond-ticks and divided once per event, so tempo changes never
    // accumulate rounding error.
    let division = u128::from(division);
    let (mut tempo, mut last_tick, mut acc) = (DEFAULT_TEMPO, 0u64, 0u128);
    let mut events = Vec::new();
    for (tick, item) in items {
        acc += u128::from(tick - last_tick) * u128::from(tempo);
        last_tick = tick;
        match item {
            Item::Tempo(t) => tempo = t,
            Item::Channel(event) => {
                let nanos = acc * 1000 / division;
                events.push(TimedEvent {
                    at: Duration::from_nanos(nanos as u64),
                    event,
                });
            }
        }
    }
    Ok(events)
}

/// Formats an event as one FluidSynth shell command, without the newline.
pub fn shell_line(e: &Event) -> String {
    match *e {
        Event::NoteOn { ch, key, vel } => format!("noteon {ch} {key} {vel}"),
        Event::NoteOff { ch, key } => format!("noteoff {ch} {key}"),
        Event::Control { ch, ctrl, value } => format!("cc {ch} {ctrl} {value}"),
        Event::Program { ch, num } => format!("prog {ch} {num}"),
        Event::PitchBend { ch, value } => format!("pitch_bend {ch} {value}"),
    }
}

fn take_chunk<'a>(rest: &mut &'a [u8]) -> Result<(&'a [u8], &'a [u8]), SmfError> {
    if rest.len() < 8 {
        return Err(SmfError::Truncated);
    }
    let len = u32::from_be_bytes([rest[4], rest[5], rest[6], rest[7]]) as usize;
    let body = rest[8..].get(..len).ok_or(SmfError::Truncated)?;
    let id = &rest[..4];
    *rest = &rest[8 + len..];
    Ok((id, body))
}

fn read_track(mut data: &[u8], items: &mut Vec<(u64, Item)>) -> Result<(), SmfError> {
    let mut tick = 0u64;
    let mut running: Option<u8> = None;
    while !data.is_empty() {
        tick += u64::from(var_len(&mut data)?);
        let before_status = data;
        let mut status = byte(&mut data)?;
        match status {
            0xFF => {
                let kind = byte(&mut data)?;
                let len = var_len(&mut data)? as usize;
                let body = take(&mut data, len)?;
                running = None;
                match kind {
                    0x2F => return Ok(()),
                    0x51 if len == 3 => {
                        let t =
                            u64::from(body[0]) << 16 | u64::from(body[1]) << 8 | u64::from(body[2]);
                        items.push((tick, Item::Tempo(t)));
                    }
                    _ => {}
                }
                continue;
            }
            0xF0 | 0xF7 => {
                let len = var_len(&mut data)? as usize;
                take(&mut data, len)?;
                running = None;
                continue;
            }
            0xF1..=0xFE => return Err(SmfError::Malformed("system message inside a track")),
            0x80..=0xEF => running = Some(status),
            _ => {
                // A data byte where a status was due is running status: the byte belongs to the message.
                status = running.ok_or(SmfError::Malformed("data byte with no running status"))?;
                data = before_status;
            }
        }
        let ch = status & 0x0F;
        let event = match status & 0xF0 {
            0x80 => {
                let (key, _) = (byte(&mut data)?, byte(&mut data)?);
                Some(Event::NoteOff { ch, key })
            }
            0x90 => {
                let (key, vel) = (byte(&mut data)?, byte(&mut data)?);
                Some(if vel == 0 {
                    Event::NoteOff { ch, key }
                } else {
                    Event::NoteOn { ch, key, vel }
                })
            }
            0xA0 => {
                take(&mut data, 2)?;
                None
            }
            0xB0 => {
                let (ctrl, value) = (byte(&mut data)?, byte(&mut data)?);
                Some(Event::Control { ch, ctrl, value })
            }
            0xC0 => Some(Event::Program {
                ch,
                num: byte(&mut data)?,
            }),
            0xD0 => {
                take(&mut data, 1)?;
                None
            }
            _ => {
                let (lsb, msb) = (byte(&mut data)?, byte(&mut data)?);
                Some(Event::PitchBend {
                    ch,
                    value: u16::from(msb) << 7 | u16::from(lsb),
                })
            }
        };
        if let Some(event) = event {
            items.push((tick, Item::Channel(event)));
        }
    }
    Ok(())
}

fn byte(data: &mut &[u8]) -> Result<u8, SmfError> {
    let (&b, rest) = data
        .split_first()
        .ok_or(SmfError::Malformed("event cut short"))?;
    *data = rest;
    Ok(b)
}

fn take<'a>(data: &mut &'a [u8], len: usize) -> Result<&'a [u8], SmfError> {
    let body = data
        .get(..len)
        .ok_or(SmfError::Malformed("event cut short"))?;
    *data = &data[len..];
    Ok(body)
}

/// A variable-length quantity: seven bits a byte, high bit set on every byte but the last, at most four bytes.
fn var_len(data: &mut &[u8]) -> Result<u32, SmfError> {
    let mut value = 0u32;
    for _ in 0..4 {
        let b = byte(data)?;
        value = value << 7 | u32::from(b & 0x7F);
        if b & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(SmfError::Malformed(
        "variable-length number longer than four bytes",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = id.to_vec();
        v.extend_from_slice(&(body.len() as u32).to_be_bytes());
        v.extend_from_slice(body);
        v
    }

    fn file(format: u16, division: u16, tracks: &[&[u8]]) -> Vec<u8> {
        let mut head = format.to_be_bytes().to_vec();
        head.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
        head.extend_from_slice(&division.to_be_bytes());
        let mut v = chunk(b"MThd", &head);
        for t in tracks {
            v.extend(chunk(b"MTrk", t));
        }
        v
    }

    const TEMPO_500000: [u8; 7] = [0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20];
    const END: [u8; 4] = [0x00, 0xFF, 0x2F, 0x00];

    fn at(ms: u64, event: Event) -> TimedEvent {
        TimedEvent {
            at: Duration::from_millis(ms),
            event,
        }
    }

    #[test]
    fn reads_tempo_and_notes() {
        let track = [
            &TEMPO_500000[..],
            &[0x00, 0x90, 60, 100, 0x83, 0x60, 0x80, 60, 64],
            &END,
        ]
        .concat();
        let events = read_smf(&file(0, 480, &[&track])).unwrap();
        assert_eq!(
            events,
            vec![
                at(
                    0,
                    Event::NoteOn {
                        ch: 0,
                        key: 60,
                        vel: 100
                    }
                ),
                at(500, Event::NoteOff { ch: 0, key: 60 }),
            ]
        );
    }

    #[test]
    fn note_on_zero_velocity_is_note_off() {
        let track = [
            &TEMPO_500000[..],
            &[0x00, 0x91, 62, 90, 0x83, 0x60, 0x91, 62, 0],
            &END,
        ]
        .concat();
        let events = read_smf(&file(0, 480, &[&track])).unwrap();
        assert_eq!(events[1], at(500, Event::NoteOff { ch: 1, key: 62 }));
    }

    #[test]
    fn shell_line_formats() {
        assert_eq!(
            shell_line(&Event::NoteOn {
                ch: 0,
                key: 60,
                vel: 100
            }),
            "noteon 0 60 100"
        );
        assert_eq!(
            shell_line(&Event::NoteOff { ch: 0, key: 60 }),
            "noteoff 0 60"
        );
        assert_eq!(
            shell_line(&Event::Control {
                ch: 2,
                ctrl: 7,
                value: 100
            }),
            "cc 2 7 100"
        );
        assert_eq!(shell_line(&Event::Program { ch: 3, num: 24 }), "prog 3 24");
        assert_eq!(
            shell_line(&Event::PitchBend { ch: 0, value: 8192 }),
            "pitch_bend 0 8192"
        );
    }

    #[test]
    fn format1_merges_tracks_and_applies_tempo_changes() {
        // Track 0: tempo 500000 at tick 0, tempo 250000 at tick 480. Track 1: a note at 0, 480 and 960.
        let conductor = [
            &TEMPO_500000[..],
            &[0x83, 0x60, 0xFF, 0x51, 0x03, 0x03, 0xD0, 0x90],
            &END,
        ]
        .concat();
        let notes = [
            &[0x00, 0x90, 60, 100][..],
            &[0x83, 0x60, 0x90, 62, 100],
            &[0x83, 0x60, 0x90, 64, 100],
            &END,
        ]
        .concat();
        let events = read_smf(&file(1, 480, &[&conductor, &notes])).unwrap();
        assert_eq!(
            events,
            vec![
                at(
                    0,
                    Event::NoteOn {
                        ch: 0,
                        key: 60,
                        vel: 100
                    }
                ),
                at(
                    500,
                    Event::NoteOn {
                        ch: 0,
                        key: 62,
                        vel: 100
                    }
                ),
                at(
                    750,
                    Event::NoteOn {
                        ch: 0,
                        key: 64,
                        vel: 100
                    }
                ),
            ]
        );
    }

    #[test]
    fn running_status_and_other_messages() {
        // Note on with running status, then control, program, pitch bend and a skipped aftertouch.
        let track = [
            &TEMPO_500000[..],
            &[0x00, 0x90, 60, 100, 0x00, 64, 100],
            &[0x00, 0xB0, 7, 90, 0x00, 0xC0, 24, 0x00, 0xA0, 60, 10],
            &[0x00, 0xE0, 0x00, 0x40],
            &END,
        ]
        .concat();
        let events: Vec<Event> = read_smf(&file(0, 480, &[&track]))
            .unwrap()
            .into_iter()
            .map(|e| e.event)
            .collect();
        assert_eq!(
            events,
            vec![
                Event::NoteOn {
                    ch: 0,
                    key: 60,
                    vel: 100
                },
                Event::NoteOn {
                    ch: 0,
                    key: 64,
                    vel: 100
                },
                Event::Control {
                    ch: 0,
                    ctrl: 7,
                    value: 90
                },
                Event::Program { ch: 0, num: 24 },
                Event::PitchBend { ch: 0, value: 8192 },
            ]
        );
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        assert!(read_smf(b"RIFF").is_err());
        let track = [&TEMPO_500000[..], &END].concat();
        assert!(read_smf(&file(0, 0xE728, &[&track])).is_err()); // SMPTE division
        let mut cut = file(0, 480, &[&track]);
        cut.truncate(cut.len() - 2);
        assert!(read_smf(&cut).is_err());
    }
}
