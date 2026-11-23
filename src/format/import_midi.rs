//! Standard MIDI File import. A file knows pitches, not strings: each
//! channel of each track becomes a guitar or bass track whose notes are
//! placed on its strings (see `import::place_notes`), and the bars follow
//! the time signature events.

use std::collections::BTreeMap;

use crate::core::{
    TICKS_PER_QUARTER, Tick,
    project::{StringedVariant, TimeSignature},
    sounds::pan_from_midi,
};

use super::{
    FormatError,
    import::{
        Import, ImportedBar, ImportedNote, ImportedSong, ImportedTrack, Report, notes_on_strings,
        push_tempo,
    },
};

/// Times are rounded to this many ticks, a 192nd of a whole note: fine
/// enough for triplets and 64ths, coarse enough to tidy played timing.
const SNAP: Tick = 20;
/// A file longer than this many bars is taken as damaged.
const MAX_BARS: usize = 20_000;
const DRUM_CHANNEL: u8 = 9;

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

/// What one channel of one MIDI track played.
#[derive(Default)]
struct Voice {
    program: Option<u8>,
    bank: (u8, u8),
    volume: Option<u8>,
    pan: Option<u8>,
    /// (start, duration, key, velocity)
    notes: Vec<(Tick, Tick, u8, u8)>,
    open: BTreeMap<u8, (Tick, u8)>,
}

#[derive(Default)]
struct MidiTrack {
    name: Option<String>,
    voices: BTreeMap<u8, Voice>,
    end: Tick,
}

fn malformed(what: &str) -> FormatError {
    FormatError::UnsupportedFormat(format!("not a usable MIDI file: {what}"))
}

pub fn import_midi(bytes: &[u8]) -> Result<Import, FormatError> {
    let mut reader = Reader { bytes, at: 0 };
    if reader.take(4) != Some(b"MThd") {
        return Err(malformed("no MThd header"));
    }
    let header_length = u32::from_be_bytes(
        reader.take(4).and_then(|b| b.try_into().ok()).ok_or_else(|| malformed("short header"))?,
    ) as usize;
    let header = reader.take(header_length).ok_or_else(|| malformed("short header"))?;
    let track_count = header.get(2..4).map(|b| u16::from_be_bytes([b[0], b[1]])).unwrap_or(0);
    let division = header.get(4..6).map(|b| u16::from_be_bytes([b[0], b[1]])).unwrap_or(0);
    if division == 0 || division & 0x8000 != 0 {
        return Err(malformed("time is counted in frames, not beats"));
    }
    let scale = |ticks: u64| -> Tick { (ticks * TICKS_PER_QUARTER as u64 / u64::from(division)) as Tick };

    let mut song = ImportedSong::default();
    let mut tempos: Vec<(Tick, f32)> = Vec::new();
    let mut signatures: Vec<(Tick, TimeSignature)> = Vec::new();
    let mut tracks: Vec<MidiTrack> = Vec::new();
    let mut seen = Seen::default();

    for _ in 0..track_count {
        let Some(tag) = reader.take(4) else { break };
        if tag != b"MTrk" {
            return Err(malformed("a track chunk is missing"));
        }
        let length = u32::from_be_bytes(
            reader.take(4).and_then(|b| b.try_into().ok()).ok_or_else(|| malformed("short chunk"))?,
        ) as usize;
        // A cut-off last chunk is read as far as it goes.
        let body = reader.bytes.get(reader.at..).unwrap_or(&[]);
        let body = &body[..length.min(body.len())];
        reader.at += body.len();
        tracks.push(read_track(
            &mut Reader { bytes: body, at: 0 },
            &scale,
            &mut tempos,
            &mut signatures,
            &mut seen,
        ));
    }
    if tracks.is_empty() {
        return Err(malformed("no tracks"));
    }
    seen.report(&mut song.report);

    // The song runs to its last note or the end the file marks, whichever
    // is later, so empty bars at the end survive.
    let end = tracks
        .iter()
        .flat_map(|track| {
            track
                .voices
                .values()
                .flat_map(|voice| voice.notes.iter().map(|&(start, duration, _, _)| start + duration))
                .chain(std::iter::once(track.end))
        })
        .max()
        .unwrap_or(0);
    song.bars = bars_until(end, &mut signatures)?;

    // The first track of a multi-track file is usually the conductor; its
    // name is the song's.
    let note_less = |track: &MidiTrack| track.voices.values().all(|voice| voice.notes.is_empty());
    if let Some(first) = tracks.first().filter(|track| note_less(track) && tracks.len() > 1) {
        song.title = first.name.clone().unwrap_or_default();
    }
    if song.title.is_empty() {
        song.title = "Imported MIDI file".to_owned();
    }

    for (index, track) in tracks.iter().enumerate() {
        let voices: Vec<(&u8, &Voice)> = track.voices.iter().filter(|(_, voice)| !voice.notes.is_empty()).collect();
        for &(channel, voice) in &voices {
            let program = voice.program.unwrap_or(0);
            let base = track
                .name
                .clone()
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("Track {}", index + 1));
            let name = if voices.len() > 1 { format!("{base} (channel {})", channel + 1) } else { base };
            let mut imported = ImportedTrack::new(name, program);
            imported.volume = voice.volume.unwrap_or(100);
            imported.pan = voice.pan.map_or(0, pan_from_midi);
            imported.bank = u16::from(voice.bank.0) << 7 | u16::from(voice.bank.1);
            if *channel == DRUM_CHANNEL {
                imported.percussion = true;
                imported.bank = 128;
                imported.name.push_str(" (percussion, not imported)");
                song.report.skip("percussion tracks (kept as empty tracks)");
            } else {
                if (32..=39).contains(&program) {
                    imported.tuning = StringedVariant::Bass.default_tuning(4);
                }
                let mut moved = 0;
                let snapped: Vec<(Tick, Tick, u8, u8)> = voice
                    .notes
                    .iter()
                    .map(|&(start, duration, key, velocity)| {
                        let (s, e) = (snap(start), snap(start + duration));
                        moved += usize::from(s != start || e != start + duration);
                        (s, (e - s).max(SNAP), key, velocity)
                    })
                    .collect();
                song.report.skip_many("notes with times rounded to the nearest 192nd note", moved);
                imported.notes = notes_on_strings(&snapped, &imported.tuning, imported.fret_count, &mut song.report);
                tidy_ends(&mut imported.notes);
            }
            song.tracks.push(imported);
        }
    }

    tempos.sort_by_key(|&(tick, _)| tick);
    for (tick, bpm) in tempos {
        push_tempo(&mut song.tempos, snap(tick), bpm, false);
    }
    Ok(song.into_import())
}

fn snap(tick: Tick) -> Tick {
    (tick + SNAP / 2).div_euclid(SNAP) * SNAP
}

/// Keeps notes from running over the next on their string.
fn tidy_ends(notes: &mut [ImportedNote]) {
    for index in 0..notes.len() {
        let next = notes[index + 1..]
            .iter()
            .find(|other| other.string_index == notes[index].string_index && other.start > notes[index].start)
            .map(|other| other.start);
        if let Some(next) = next {
            notes[index].duration = notes[index].duration.min(next - notes[index].start).max(1);
        }
    }
}

/// Bars covering `end`, changing signature at the first bar line after
/// each time signature event.
fn bars_until(end: Tick, signatures: &mut [(Tick, TimeSignature)]) -> Result<Vec<ImportedBar>, FormatError> {
    signatures.sort_by_key(|&(tick, _)| tick);
    let mut bars = Vec::new();
    let mut cursor: Tick = 0;
    let mut signature = TimeSignature::new(4, 4);
    while cursor < end || bars.is_empty() {
        if let Some(&(_, found)) = signatures.iter().rev().find(|(tick, _)| *tick <= cursor) {
            signature = found;
        }
        bars.push(ImportedBar {
            signature,
            repeat_open: false,
            repeat_close: 0,
        });
        cursor += signature.measure_ticks();
        if bars.len() > MAX_BARS {
            return Err(malformed("it would need an absurd number of bars"));
        }
    }
    Ok(bars)
}

/// What a file held that is not read, counted once per kind.
#[derive(Default)]
struct Seen {
    bends: bool,
    controllers: bool,
    lyrics: bool,
    pressure: bool,
}

impl Seen {
    fn report(&self, report: &mut Report) {
        for (seen, what) in [
            (self.bends, "pitch bends"),
            (self.controllers, "controller changes (pedal, modulation, expression)"),
            (self.lyrics, "lyrics and text events"),
            (self.pressure, "aftertouch"),
        ] {
            if seen {
                report.skip(what);
            }
        }
    }
}

fn read_track(
    reader: &mut Reader,
    scale: &impl Fn(u64) -> Tick,
    tempos: &mut Vec<(Tick, f32)>,
    signatures: &mut Vec<(Tick, TimeSignature)>,
    seen: &mut Seen,
) -> MidiTrack {
    let mut track = MidiTrack::default();
    let mut raw: u64 = 0;
    let mut status: u8 = 0;
    while let Some(delta) = reader.variable_length() {
        raw += u64::from(delta);
        let now = scale(raw);
        track.end = now;
        let Some(first) = reader.byte() else { break };
        let (kind, data1) = if first & 0x80 != 0 {
            if first < 0xf0 {
                status = first;
            }
            (first, None)
        } else {
            (status, Some(first))
        };
        match kind {
            0xff => {
                let (Some(meta), Some(length)) = (reader.byte(), reader.variable_length()) else { break };
                let Some(payload) = reader.take(length as usize) else { break };
                match (meta, payload) {
                    (0x03, text) if track.name.is_none() => {
                        track.name = Some(String::from_utf8_lossy(text).trim().to_owned());
                    }
                    (0x01 | 0x05, _) => seen.lyrics = true,
                    (0x51, [a, b, c]) => {
                        let microseconds = u32::from_be_bytes([0, *a, *b, *c]).max(1);
                        tempos.push((now, 60_000_000.0 / microseconds as f32));
                    }
                    (0x58, [numerator, power, ..]) if *power < 8 => {
                        signatures.push((now, TimeSignature::new((*numerator).max(1), 1 << power)));
                    }
                    (0x2f, _) => break,
                    _ => {}
                }
            }
            0xf0 | 0xf7 => {
                let Some(length) = reader.variable_length() else { break };
                if reader.take(length as usize).is_none() {
                    break;
                }
            }
            kind if kind & 0x80 != 0 => {
                let channel = kind & 0x0f;
                let needs_two = !matches!(kind & 0xf0, 0xc0 | 0xd0);
                let Some(a) = data1.or_else(|| reader.byte()) else { break };
                let b = if needs_two {
                    let Some(b) = reader.byte() else { break };
                    b
                } else {
                    0
                };
                let voice = track.voices.entry(channel).or_default();
                match kind & 0xf0 {
                    0x90 if b > 0 => {
                        // A note struck again before its end releases the first.
                        if let Some((start, velocity)) = voice.open.insert(a, (now, b)) {
                            voice.notes.push((start, (now - start).max(1), a, velocity));
                        }
                    }
                    0x80 | 0x90 => {
                        if let Some((start, velocity)) = voice.open.remove(&a) {
                            voice.notes.push((start, (now - start).max(1), a, velocity));
                        }
                    }
                    0xb0 => match a {
                        7 => voice.volume = voice.volume.or(Some(b)),
                        10 => voice.pan = voice.pan.or(Some(b)),
                        0 => voice.bank.0 = b,
                        32 => voice.bank.1 = b,
                        _ => seen.controllers = true,
                    },
                    0xc0 => voice.program = voice.program.or(Some(a)),
                    0xe0 => seen.bends = true,
                    _ => seen.pressure = true,
                }
            }
            _ => break,
        }
    }
    // Notes still sounding when the track ends stop there.
    let end = track.end;
    for voice in track.voices.values_mut() {
        for (key, (start, velocity)) in std::mem::take(&mut voice.open) {
            voice.notes.push((start, (end - start).max(1), key, velocity));
        }
        voice.notes.sort_by_key(|&(start, _, key, _)| (start, key));
    }
    track
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{BwxProject, TICKS_PER_QUARTER as Q};
    use crate::format::{midi::write_midi, score};

    fn smf(division: u16, tracks: &[Vec<u8>]) -> Vec<u8> {
        let mut file = b"MThd".to_vec();
        file.extend(6_u32.to_be_bytes());
        file.extend(1_u16.to_be_bytes());
        file.extend((tracks.len() as u16).to_be_bytes());
        file.extend(division.to_be_bytes());
        for track in tracks {
            file.extend(b"MTrk");
            file.extend((track.len() as u32).to_be_bytes());
            file.extend(track);
        }
        file
    }

    fn keys(project: &BwxProject, index: usize) -> Vec<(Tick, Tick, u8)> {
        let track = &project.tracks[index];
        track
            .notes
            .iter()
            .filter(|n| !track.is_tied_continuation(n))
            .map(|n| (n.abs_tick, n.duration_ticks, track.midi_key_for(n)))
            .collect()
    }

    #[test]
    fn the_exported_demo_comes_back_with_its_pitches_and_times() {
        let project = score::aligned_demo();
        let back = import_midi(&write_midi(&project).unwrap()).unwrap().project;
        assert_eq!(back.tracks.len(), project.tracks.len());
        for index in 0..project.tracks.len() {
            let mut before = keys(&project, index);
            let mut after = keys(&back, index);
            before.sort();
            after.sort();
            // Ties are joined in the file, so compare where each note starts.
            let starts = |list: &[(Tick, Tick, u8)]| -> Vec<(Tick, u8)> {
                let mut starts: Vec<(Tick, u8)> = list.iter().map(|&(s, _, k)| (s, k)).collect();
                starts.dedup();
                starts
            };
            assert_eq!(starts(&before), starts(&after), "track {index}");
        }
        assert_eq!(back.tempo_points[0].bpm.round(), project.tempo_points[0].bpm.round());
        assert_eq!(back.tracks[0].bar_count, project.tracks[0].bar_count);
    }

    #[test]
    fn running_status_tempo_signature_program_and_the_drum_channel() {
        let division = 480;
        let conductor = vec![
            0x00, 0xff, 0x03, 0x04, b'S', b'o', b'n', b'g', // name
            0x00, 0xff, 0x58, 0x04, 3, 2, 24, 8, // 3/4
            0x00, 0xff, 0x51, 0x03, 0x07, 0xa1, 0x20, // 120 bpm
            0x00, 0xff, 0x2f, 0x00,
        ];
        let guitar = vec![
            0x00, 0xff, 0x03, 0x03, b'G', b't', b'r',
            0x00, 0xc0, 25, // program on channel 1
            0x00, 0x90, 64, 100, // E4 on
            0x83, 0x60, 64, 0, // 480 ticks later: off by velocity 0
            0x00, 62, 90, // running status: D4 on
            0x81, 0x70, 62, 0, // after 240 ticks: off
            0x00, 0xff, 0x2f, 0x00,
        ];
        let kit = vec![0x00, 0x99, 36, 100, 0x60, 0x89, 36, 0, 0x00, 0xff, 0x2f, 0x00];
        let import = import_midi(&smf(division, &[conductor, guitar, kit])).unwrap();
        let project = &import.project;
        assert_eq!(project.title, "Song");
        assert_eq!(project.tracks.len(), 2);
        let guitar = &project.tracks[0];
        assert_eq!((guitar.name.as_str(), guitar.program), ("Gtr", 25));
        assert_eq!(guitar.time_signature_changes[0].signature, TimeSignature::new(3, 4));
        let notes: Vec<(Tick, Tick, usize, u8)> = guitar
            .notes
            .iter()
            .map(|n| (n.abs_tick, n.duration_ticks, n.string_index, n.fret))
            .collect();
        assert_eq!(notes, [(0, Q, 0, 0), (Q, Q / 2, 1, 3)]);
        assert!(project.tracks[1].name.contains("percussion"));
        assert!(project.tracks[1].notes.is_empty());
        assert_eq!(project.tempo_points[0].bpm, 120.0);
    }

    #[test]
    fn bass_programs_get_a_bass_tuning_and_low_notes_fit() {
        let track = vec![
            0x00, 0xc0, 33, 0x00, 0x90, 28, 100, 0x60, 0x80, 28, 0, 0x00, 0xff, 0x2f, 0x00,
        ];
        let project = import_midi(&smf(96, &[track])).unwrap().project;
        let bass = &project.tracks[0];
        assert_eq!(bass.tuning.len(), 4);
        assert_eq!((bass.notes[0].string_index, bass.notes[0].fret), (3, 0));
    }

    #[test]
    fn what_is_not_read_is_reported() {
        let track = vec![
            0x00, 0xb0, 64, 127, // sustain pedal
            0x00, 0xe0, 0, 80, // pitch bend
            0x00, 0x90, 64, 100, 0x60, 0x80, 64, 0, 0x00, 0xff, 0x2f, 0x00,
        ];
        let report = import_midi(&smf(96, &[track])).unwrap().report.join("; ");
        assert!(report.contains("pitch bends") && report.contains("controller changes"));
    }

    #[test]
    fn files_that_are_not_midi_are_refused() {
        assert!(import_midi(b"RIFF....").is_err());
        assert!(import_midi(b"MThd\0\0\0\x06\0\x01\0\x01\x80\x19").is_err());
    }

    #[test]
    fn a_truncated_file_still_gives_what_was_read() {
        let track = vec![0x00, 0x90, 64, 100, 0x60, 0x80, 64, 0];
        let mut file = smf(96, &[track]);
        file.truncate(file.len() - 2);
        assert!(import_midi(&file).is_ok());
    }
}
