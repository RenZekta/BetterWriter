//! Reading and writing the notes of a Standard MIDI File, the format other
//! programs take for MIDI clips.

use super::project::{TICKS_PER_QUARTER, Tick};

/// One sounding note, positioned in ticks of `TICKS_PER_QUARTER`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmfNote {
    pub start: Tick,
    pub duration: Tick,
    pub key: u8,
    pub velocity: u8,
}

pub(crate) fn push_variable_length(out: &mut Vec<u8>, mut value: u32) {
    let mut bytes = vec![(value & 0x7f) as u8];
    value >>= 7;
    while value > 0 {
        bytes.push((value & 0x7f) as u8 | 0x80);
        value >>= 7;
    }
    bytes.reverse();
    out.extend(bytes);
}

/// A format 0 file holding `notes` on one channel at a steady `bpm`.
pub fn write_notes(notes: &[SmfNote], bpm: f32) -> Vec<u8> {
    // (tick, order, status data): note offs sort before note ons at one tick
    // so a repeated key retriggers instead of being cut short.
    let mut events: Vec<(Tick, u8, [u8; 3])> = Vec::with_capacity(notes.len() * 2);
    for note in notes {
        let key = note.key.min(127);
        events.push((note.start, 1, [0x90, key, note.velocity.clamp(1, 127)]));
        events.push((note.start + note.duration.max(1), 0, [0x80, key, 0]));
    }
    events.sort_by_key(|&(tick, order, data)| (tick, order, data[1]));

    let microseconds = (60_000_000.0 / f64::from(bpm.max(1.0))).round() as u32;
    let mut track: Vec<u8> = vec![0x00, 0xff, 0x51, 0x03];
    track.extend(&microseconds.to_be_bytes()[1..]);
    let mut last = 0;
    for (tick, _, data) in events {
        push_variable_length(&mut track, (tick - last).max(0) as u32);
        last = tick;
        track.extend(data);
    }
    track.extend([0x00, 0xff, 0x2f, 0x00]);

    let mut file: Vec<u8> = b"MThd".to_vec();
    file.extend(6_u32.to_be_bytes());
    file.extend(0_u16.to_be_bytes());
    file.extend(1_u16.to_be_bytes());
    file.extend((TICKS_PER_QUARTER as u16).to_be_bytes());
    file.extend(b"MTrk");
    file.extend((track.len() as u32).to_be_bytes());
    file.extend(track);
    file
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        let value = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(value)
    }

    fn take(&mut self, count: usize) -> Option<&[u8]> {
        let slice = self.bytes.get(self.at..self.at.checked_add(count)?)?;
        self.at += count;
        Some(slice)
    }

    fn variable_length(&mut self) -> Option<u32> {
        let mut value: u32 = 0;
        for _ in 0..4 {
            let byte = self.byte()?;
            value = value << 7 | u32::from(byte & 0x7f);
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }
}

/// The notes of every track of a format 0 or 1 file, earliest first. `None`
/// when the bytes are not such a file or it counts time in frames.
pub fn read_notes(bytes: &[u8]) -> Option<Vec<SmfNote>> {
    let mut reader = Reader { bytes, at: 0 };
    if reader.take(4)? != b"MThd" {
        return None;
    }
    let header_length = u32::from_be_bytes(reader.take(4)?.try_into().ok()?) as usize;
    let header = reader.take(header_length)?;
    let tracks = u16::from_be_bytes(header.get(2..4)?.try_into().ok()?);
    let division = u16::from_be_bytes(header.get(4..6)?.try_into().ok()?);
    if division == 0 || division & 0x8000 != 0 {
        return None;
    }
    let scale = |ticks: u64| -> Tick {
        (ticks * TICKS_PER_QUARTER as u64 / u64::from(division)) as Tick
    };

    let mut notes: Vec<SmfNote> = Vec::new();
    for _ in 0..tracks {
        if reader.take(4)? != b"MTrk" {
            return None;
        }
        let length = u32::from_be_bytes(reader.take(4)?.try_into().ok()?) as usize;
        let mut track = Reader {
            bytes: reader.take(length)?,
            at: 0,
        };
        read_track(&mut track, &scale, &mut notes)?;
    }
    notes.sort_by_key(|note| (note.start, note.key));
    Some(notes)
}

fn read_track(
    track: &mut Reader,
    scale: &dyn Fn(u64) -> Tick,
    notes: &mut Vec<SmfNote>,
) -> Option<()> {
    // Held notes by (channel, key): start tick and velocity.
    let mut held: Vec<((u8, u8), u64, u8)> = Vec::new();
    let mut now: u64 = 0;
    let mut status: u8 = 0;
    let mut finish = |held: &mut Vec<((u8, u8), u64, u8)>, channel: u8, key: u8, at: u64| {
        if let Some(index) = held.iter().position(|(id, ..)| *id == (channel, key)) {
            let (_, start, velocity) = held.remove(index);
            let (from, to) = (scale(start), scale(at));
            notes.push(SmfNote {
                start: from,
                duration: (to - from).max(1),
                key,
                velocity,
            });
        }
    };
    while track.at < track.bytes.len() {
        now += u64::from(track.variable_length()?);
        let first = track.byte()?;
        let data_first = if first & 0x80 != 0 {
            status = first;
            None
        } else {
            Some(first)
        };
        match status {
            0xff => {
                track.byte()?;
                let length = track.variable_length()? as usize;
                track.take(length)?;
            }
            0xf0 | 0xf7 => {
                let length = track.variable_length()? as usize;
                track.take(length)?;
            }
            0x80..=0xef => {
                let channel = status & 0x0f;
                let wide = !matches!(status & 0xf0, 0xc0 | 0xd0);
                let one = match data_first {
                    Some(value) => value,
                    None => track.byte()?,
                };
                let two = if wide { track.byte()? } else { 0 };
                match status & 0xf0 {
                    0x90 if two > 0 => {
                        // A key struck again while held ends the earlier note.
                        finish(&mut held, channel, one, now);
                        held.push(((channel, one), now, two));
                    }
                    0x80 | 0x90 => finish(&mut held, channel, one, now),
                    _ => {}
                }
            }
            _ => return None,
        }
    }
    for ((channel, key), _, _) in held.clone() {
        finish(&mut held, channel, key, now);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q: Tick = TICKS_PER_QUARTER;

    fn note(start: Tick, duration: Tick, key: u8) -> SmfNote {
        SmfNote {
            start,
            duration,
            key,
            velocity: 90,
        }
    }

    #[test]
    fn written_notes_read_back() {
        let notes = [note(0, Q, 40), note(Q, Q / 2, 45), note(Q, Q, 52)];
        let file = write_notes(&notes, 120.0);
        assert_eq!(&file[..4], b"MThd");
        assert_eq!(read_notes(&file).unwrap(), [notes[0], notes[1], notes[2]]);
    }

    #[test]
    fn a_repeated_key_retriggers_instead_of_being_cut() {
        let notes = [note(0, Q, 40), note(Q, Q, 40)];
        let read = read_notes(&write_notes(&notes, 90.0)).unwrap();
        assert_eq!(read, notes);
    }

    #[test]
    fn the_tempo_is_written_in_microseconds_per_quarter() {
        let file = write_notes(&[note(0, Q, 40)], 120.0);
        let at = file
            .windows(3)
            .position(|window| window == [0xff, 0x51, 0x03])
            .unwrap();
        assert_eq!(&file[at + 3..at + 6], &[0x07, 0xa1, 0x20]);
    }

    #[test]
    fn other_resolutions_are_scaled_and_running_status_is_followed() {
        // 480 ticks per quarter. The second note ends with a note on of
        // velocity 0 that reuses the previous status byte.
        let mut track: Vec<u8> = vec![0x00, 0x90, 60, 100, 0x83, 0x60, 0x80, 60, 0];
        track.extend([0x00, 0x90, 62, 100, 0x81, 0x70, 62, 0]);
        track.extend([0x00, 0xff, 0x2f, 0x00]);
        let mut file: Vec<u8> = b"MThd".to_vec();
        file.extend(6_u32.to_be_bytes());
        file.extend([0, 1, 0, 1, 0x01, 0xe0]);
        file.extend(b"MTrk");
        file.extend((track.len() as u32).to_be_bytes());
        file.extend(track);
        let notes = read_notes(&file).unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0], SmfNote { start: 0, duration: Q, key: 60, velocity: 100 });
        assert_eq!(notes[1], SmfNote { start: Q, duration: Q / 2, key: 62, velocity: 100 });
    }

    #[test]
    fn notes_still_held_at_the_end_stop_there() {
        let track: Vec<u8> = vec![0x00, 0x90, 60, 100, 0x81, 0x70, 0xff, 0x2f, 0x00];
        let mut file: Vec<u8> = b"MThd".to_vec();
        file.extend(6_u32.to_be_bytes());
        file.extend([0, 0, 0, 1, 0x01, 0xe0]);
        file.extend(b"MTrk");
        file.extend((track.len() as u32).to_be_bytes());
        file.extend(track);
        let notes = read_notes(&file).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].duration, 240 * 2);
    }

    #[test]
    fn garbage_and_frame_based_files_are_refused() {
        assert_eq!(read_notes(b"RIFF....."), None);
        assert_eq!(read_notes(&[]), None);
        let mut file = write_notes(&[note(0, Q, 40)], 120.0);
        file[12] = 0xe7;
        assert_eq!(read_notes(&file), None);
        let cut = &write_notes(&[note(0, Q, 40)], 120.0)[..20];
        assert_eq!(read_notes(cut), None);
    }
}
