use std::io::{Cursor, Read};

use crate::core::{
    BwxProject, NoteEffects, TICKS_PER_QUARTER, Tick,
    project::{InstrumentTrack, TabNote, TieOrigin, TimeSignature, TimeSignatureChange},
    length::{detect_length, dotted_ticks},
    sounds::{melodic_channel, midi_pan, pan_from_midi, variant_for_program},
};

use super::{
    FormatError,
    import::{Import, Report, scaled},
    score::{self, ExportBar},
};

/// TuxGuitar compares this header text exactly; its own is "1.5", not "1.5.0".
const TG_VERSION: &str = "TuxGuitar File Format - 1.5";
const TG_START_OFFSET: Tick = TICKS_PER_QUARTER;

const TRACK_SOLO: u8 = 0x01;
const TRACK_MUTE: u8 = 0x02;
const TRACK_LYRICS: u8 = 0x04;

const MEASURE_HEADER_TIMESIGNATURE: u8 = 0x01;
const MEASURE_HEADER_TEMPO: u8 = 0x02;
const MEASURE_HEADER_REPEAT_OPEN: u8 = 0x04;
const MEASURE_HEADER_REPEAT_CLOSE: u8 = 0x08;
const MEASURE_HEADER_REPEAT_ALTERNATIVE: u8 = 0x10;
const MEASURE_HEADER_MARKER: u8 = 0x20;
const MEASURE_HEADER_TRIPLET_FEEL: u8 = 0x40;

const MEASURE_CLEF: u8 = 0x01;
const MEASURE_KEYSIGNATURE: u8 = 0x02;

const BEAT_HAS_NEXT: u8 = 0x01;
const BEAT_HAS_STROKE: u8 = 0x02;
const BEAT_HAS_CHORD: u8 = 0x04;
const BEAT_HAS_TEXT: u8 = 0x08;
const BEAT_HAS_VOICE: u8 = 0x10;
const BEAT_HAS_VOICE_CHANGES: u8 = 0x20;

const VOICE_HAS_NOTES: u8 = 0x01;
const VOICE_NEXT_DURATION: u8 = 0x02;
const VOICE_DIRECTION_UP: u8 = 0x04;
const VOICE_DIRECTION_DOWN: u8 = 0x08;

const NOTE_HAS_NEXT: u8 = 0x01;
const NOTE_TIED: u8 = 0x02;
const NOTE_EFFECT: u8 = 0x04;
const NOTE_VELOCITY: u8 = 0x08;

const DURATION_DOTTED: u8 = 0x01;
const DURATION_DOUBLE_DOTTED: u8 = 0x02;
const DURATION_NO_TUPLET: u8 = 0x04;

const EFFECT_BEND: u32 = 0x000001;
const EFFECT_TREMOLO_BAR: u32 = 0x000002;
const EFFECT_HARMONIC: u32 = 0x000004;
const EFFECT_GRACE: u32 = 0x000008;
const EFFECT_TRILL: u32 = 0x000010;
const EFFECT_TREMOLO_PICKING: u32 = 0x000020;
const EFFECT_VIBRATO: u32 = 0x000040;
const EFFECT_DEAD: u32 = 0x000080;
const EFFECT_SLIDE: u32 = 0x000100;
const EFFECT_HAMMER: u32 = 0x000200;
const EFFECT_GHOST: u32 = 0x000400;
const EFFECT_ACCENTUATED: u32 = 0x000800;
const EFFECT_HEAVY_ACCENTUATED: u32 = 0x001000;
const EFFECT_PALM_MUTE: u32 = 0x002000;
const EFFECT_STACCATO: u32 = 0x004000;
const EFFECT_TAPPING: u32 = 0x008000;
const EFFECT_SLAPPING: u32 = 0x010000;
const EFFECT_POPPING: u32 = 0x020000;
const EFFECT_FADE_IN: u32 = 0x040000;
const EFFECT_LET_RING: u32 = 0x080000;

#[derive(Clone, Copy)]
struct TgDuration {
    value: u8,
    ticks: Tick,
}

impl TgDuration {
    fn quarter() -> Self {
        Self {
            value: 4,
            ticks: TICKS_PER_QUARTER,
        }
    }
}

#[derive(Clone, Copy)]
struct VoiceData {
    start: Tick,
    duration: TgDuration,
    velocity: u8,
    flags: u8,
}

impl VoiceData {
    fn new(measure_start: Tick) -> Self {
        Self {
            start: measure_start,
            duration: TgDuration::quarter(),
            velocity: 95,
            flags: 0,
        }
    }
}

#[derive(Clone)]
struct MeasureHeader {
    start: Tick,
    signature: TimeSignature,
    tempo: u16,
    repeat_open: bool,
    repeat_close: u16,
}

#[cfg(test)]
pub fn read_tg(bytes: &[u8]) -> Result<BwxProject, FormatError> {
    import_tg(bytes).map(|import| import.project)
}

/// Reads a TuxGuitar file, with what it held that the project cannot keep.
pub fn import_tg(bytes: &[u8]) -> Result<Import, FormatError> {
    let mut reader = TgReader::new(bytes);
    let version = reader.read_ubyte_string()?;
    if !version.starts_with("TuxGuitar File Format") {
        return Err(FormatError::UnsupportedFormat(format!(
            "not a TuxGuitar stream: {version}"
        )));
    }

    let title = reader.read_ubyte_string()?;
    let _artist = reader.read_ubyte_string()?;
    let _album = reader.read_ubyte_string()?;
    let _author = reader.read_ubyte_string()?;
    let _date = reader.read_ubyte_string()?;
    let _copyright = reader.read_ubyte_string()?;
    let _writer = reader.read_ubyte_string()?;
    let _transcriber = reader.read_ubyte_string()?;
    let _comments = reader.read_i32_string()?;

    let channel_count = reader.read_u8()? as usize;
    let mut channel_programs = Vec::with_capacity(channel_count);
    let mut channel_banks = Vec::with_capacity(channel_count);
    let mut channel_mixes = Vec::with_capacity(channel_count);
    let mut channel_names = Vec::with_capacity(channel_count);
    for _ in 0..channel_count {
        let _channel_id = reader.read_i16()? as u16;
        let bank = reader.read_u8()?;
        let program = reader.read_u8()?;
        let volume = reader.read_u8()?;
        let balance = reader.read_u8()?;
        let _chorus = reader.read_u8()?;
        let _reverb = reader.read_u8()?;
        let _phaser = reader.read_u8()?;
        let _tremolo = reader.read_u8()?;
        let name = reader.read_ubyte_string()?;
        let params = reader.read_i16()? as usize;
        for _ in 0..params {
            let _key = reader.read_ubyte_string()?;
            let _value = reader.read_i32_string()?;
        }
        channel_programs.push(program);
        channel_banks.push(bank);
        channel_mixes.push((volume.min(127), pan_from_midi(balance)));
        channel_names.push(name);
    }

    let header_count = reader.read_i16()?.max(0) as usize;
    let mut headers = Vec::with_capacity(header_count);
    let mut start = TG_START_OFFSET;
    let mut last_signature = TimeSignature::new(4, 4);
    let mut last_tempo = 120;
    for _ in 0..header_count {
        let flags = reader.read_u8()?;
        if flags & MEASURE_HEADER_TIMESIGNATURE != 0 {
            last_signature = reader.read_time_signature()?;
        }
        if flags & MEASURE_HEADER_TEMPO != 0 {
            last_tempo = reader.read_i16()?.max(1) as u16;
        }
        let repeat_open = flags & MEASURE_HEADER_REPEAT_OPEN != 0;
        let mut repeat_close = 0;
        if flags & MEASURE_HEADER_REPEAT_CLOSE != 0 {
            repeat_close = reader.read_i16()?.max(0) as u16;
        }
        if flags & MEASURE_HEADER_REPEAT_ALTERNATIVE != 0 {
            let _ = reader.read_u8()?;
            reader.report.skip("alternate endings");
        }
        if flags & MEASURE_HEADER_MARKER != 0 {
            reader.read_marker()?;
            reader.report.skip("section markers");
        }
        if flags & MEASURE_HEADER_TRIPLET_FEEL != 0 {
            let _ = reader.read_u8()?;
            reader.report.skip("triplet feel marks");
        }
        let length = last_signature.measure_ticks();
        headers.push(MeasureHeader {
            start,
            signature: last_signature,
            tempo: last_tempo,
            repeat_open,
            repeat_close,
        });
        start += length;
    }

    let track_count = reader.read_u8()? as usize;
    let mut tracks = Vec::with_capacity(track_count);
    let mut next_note_id = 1;

    for track_index in 0..track_count {
        let track_flags = reader.read_u8()?;
        let name = reader.read_ubyte_string()?;
        let channel_id = reader.read_i16()?.max(1) as usize;
        let percussion = channel_banks.get(channel_id.saturating_sub(1)) == Some(&128);
        let program = channel_programs
            .get(channel_id.saturating_sub(1))
            .copied()
            .unwrap_or(24);
        let (volume, pan) = channel_mixes
            .get(channel_id.saturating_sub(1))
            .copied()
            .unwrap_or((100, 0));
        let mut notes = Vec::new();
        let mut texts = Vec::new();

        for header in &headers {
            reader.read_measure(header, &mut notes, &mut texts, &mut next_note_id)?;
        }

        let string_count = reader.read_u8()? as usize;
        let mut tuning = Vec::with_capacity(string_count);
        for _ in 0..string_count {
            tuning.push(reader.read_u8()?);
        }
        // The file keeps the track's offset (capo or transposition, in
        // semitones) plus 24, so 24 means none.
        let offset = i16::from(reader.read_u8()?) - 24;
        let _r = reader.read_u8()?;
        let _g = reader.read_u8()?;
        let _b = reader.read_u8()?;
        if track_flags & TRACK_LYRICS != 0 {
            let _from = reader.read_i16()?;
            let _lyrics = reader.read_i32_string()?;
            reader.report.skip("lyrics");
        }

        // Frets are counted from the capo.
        for pitch in &mut tuning {
            *pitch = (i16::from(*pitch) + offset).clamp(0, 127) as u8;
        }
        if percussion {
            notes.clear();
            reader.report.skip("percussion tracks (kept as empty tracks)");
        }
        tracks.push(InstrumentTrack {
            id: (track_index + 1) as u64,
            name: {
                let base = if name.is_empty() {
                    channel_names
                        .get(track_index)
                        .cloned()
                        .unwrap_or_else(|| format!("Track {}", track_index + 1))
                } else {
                    name
                };
                if percussion {
                    format!("{base} (percussion, not imported)")
                } else {
                    base
                }
            },
            channel: melodic_channel(track_index),
            program,
            bank: if percussion { 128 } else { 0 },
            sound_auto: false,
            instrument_id: String::new(),
            solo: track_flags & TRACK_SOLO != 0,
            mute: track_flags & TRACK_MUTE != 0,
            volume,
            pan,
            tuning: if tuning.is_empty() {
                vec![64, 59, 55, 50, 45, 40]
            } else {
                tuning
            },
            fret_count: 24,
            time_signature_changes: signature_changes(&headers),
            notes,
            bar_count: (headers.len() as u32).max(1),
            instrument_family: crate::core::project::InstrumentFamily::Stringed,
            stringed_variant: variant_for_program(program),
            bar_marks: Vec::new(),
            sound_chain: Vec::new(),
            fx_bypassed: false,
            tempo_points: Vec::new(),
            volume_points: Vec::new(),
            pan_points: Vec::new(),
            tuplets: Vec::new(),
            annotations: Vec::new(),
        });
        if let Some(track) = tracks.last_mut() {
            for (tick, text) in &texts {
                track.set_annotation(*tick, text);
            }
            for header in &headers {
                let at = header.start - TG_START_OFFSET;
                if header.repeat_open {
                    track.set_repeat_open(at, true);
                }
                if header.repeat_close > 0 {
                    track.set_repeat_close(at, header.repeat_close);
                }
            }
        }
    }

    let mut project = BwxProject {
        schema_version: 1,
        title: if title.is_empty() {
            "Imported TuxGuitar score".to_owned()
        } else {
            title
        },
        tempo_points: tempo_points(&headers),
        volume_points: Vec::new(),
        pan_points: Vec::new(),
        per_track_automation: false,
        tracks,
        next_note_id,
        master_chain: Vec::new(),
        master_fx_bypassed: false,
    };
    // TuxGuitar's measure headers are shared across all tracks, so bar_count
    // should already cover every note; this is just a safety net against a
    // malformed import leaving notes past the last declared bar.
    project.normalize_bar_counts();
    Ok(Import {
        project,
        report: reader.report.lines(),
    })
}

pub fn write_tg(project: &BwxProject) -> Result<Vec<u8>, FormatError> {
    let song = score::prepare(project)?;
    let mut writer = TgWriter::default();
    writer.write_ubyte_string(TG_VERSION)?;
    writer.write_ubyte_string(&project.title)?;
    for value in ["", "", "", "", "", "BetterWriter", ""] {
        writer.write_ubyte_string(value)?;
    }
    writer.write_i32_string("Generated by BetterWriter")?;

    writer.write_u8(project.tracks.len().min(255) as u8);
    for (index, track) in project.tracks.iter().enumerate() {
        let (bank, program) = track.sound();
        writer.write_i16((index + 1).min(i16::MAX as usize) as i16);
        writer.write_u8(if bank < 128 { bank as u8 } else { 0 });
        writer.write_u8(program.min(127));
        writer.write_u8(track.volume.min(127));
        writer.write_u8(midi_pan(track.pan));
        writer.write_u8(0);
        writer.write_u8(24);
        writer.write_u8(0);
        writer.write_u8(0);
        writer.write_ubyte_string(&track.name)?;
        writer.write_i16(0);
    }

    writer.write_i16(song.grid.len().min(i16::MAX as usize) as i16);
    let mut last_signature = None;
    let mut last_tempo = None;
    for (index, bar) in song.grid.iter().enumerate() {
        let tempo = song.bar_tempo(index);
        let mut flags = 0;
        if last_signature != Some(bar.signature) {
            flags |= MEASURE_HEADER_TIMESIGNATURE;
        }
        if last_tempo != Some(tempo) {
            flags |= MEASURE_HEADER_TEMPO;
        }
        if bar.repeat_open {
            flags |= MEASURE_HEADER_REPEAT_OPEN;
        }
        if bar.repeat_close > 0 {
            flags |= MEASURE_HEADER_REPEAT_CLOSE;
        }
        writer.write_u8(flags);
        if flags & MEASURE_HEADER_TIMESIGNATURE != 0 {
            writer.write_time_signature(bar.signature);
        }
        if flags & MEASURE_HEADER_TEMPO != 0 {
            writer.write_i16(tempo as i16);
        }
        if flags & MEASURE_HEADER_REPEAT_CLOSE != 0 {
            writer.write_i16(bar.repeat_close.min(i16::MAX as u16) as i16);
        }
        last_signature = Some(bar.signature);
        last_tempo = Some(tempo);
    }

    writer.write_u8(project.tracks.len().min(255) as u8);
    for (track_index, exported) in song.tracks.iter().enumerate() {
        let track = exported.track;
        let mut flags = 0;
        if track.solo {
            flags |= TRACK_SOLO;
        }
        if track.mute {
            flags |= TRACK_MUTE;
        }
        writer.write_u8(flags);
        writer.write_ubyte_string(&track.name)?;
        writer.write_i16((track_index + 1).min(i16::MAX as usize) as i16);
        for (measure_index, bar) in exported.bars.iter().enumerate() {
            writer.write_measure(exported, bar, song.grid[measure_index].start, measure_index == 0);
        }
        writer.write_u8(track.tuning.len().min(255) as u8);
        for string in &track.tuning {
            writer.write_u8(*string);
        }
        writer.write_u8(24);
        writer.write_u8(120);
        writer.write_u8(120);
        writer.write_u8(120);
    }

    Ok(writer.into_inner())
}

fn signature_changes(headers: &[MeasureHeader]) -> Vec<TimeSignatureChange> {
    let mut changes = Vec::new();
    let mut last = None;
    for header in headers {
        if last != Some(header.signature) {
            changes.push(TimeSignatureChange {
                at_tick: (header.start - TG_START_OFFSET).max(0),
                signature: header.signature,
            });
            last = Some(header.signature);
        }
    }
    if changes.is_empty() {
        changes.push(TimeSignatureChange {
            at_tick: 0,
            signature: TimeSignature::new(4, 4),
        });
    }
    changes
}

/// Mirrors `signature_changes`: TuxGuitar's measure headers carry a tempo
/// per measure, so a tempo automation point is emitted wherever that value
/// changes from the previous measure.
fn tempo_points(headers: &[MeasureHeader]) -> Vec<crate::core::TempoPoint> {
    let mut points = Vec::new();
    let mut last = None;
    for header in headers {
        if last != Some(header.tempo) {
            points.push(crate::core::TempoPoint::new(
                (header.start - TG_START_OFFSET).max(0),
                header.tempo as f32,
            ));
            last = Some(header.tempo);
        }
    }
    if points.is_empty() {
        points.push(crate::core::TempoPoint::new(0, 120.0));
    }
    points
}

struct TgReader<'a> {
    cursor: Cursor<&'a [u8]>,
    report: Report,
}

impl<'a> TgReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(bytes),
            report: Report::default(),
        }
    }

    fn read_measure(
        &mut self,
        header: &MeasureHeader,
        notes: &mut Vec<TabNote>,
        texts: &mut Vec<(Tick, String)>,
        next_note_id: &mut u64,
    ) -> Result<(), FormatError> {
        let flags = self.read_u8()?;
        let mut voices = [VoiceData::new(header.start), VoiceData::new(header.start)];
        let mut beat_header = BEAT_HAS_NEXT;
        let mut current = header.start;
        while beat_header & BEAT_HAS_NEXT != 0 {
            beat_header = self.read_u8()?;
            let beat_start = next_beat_start(&voices, &mut current);
            for (voice_index, voice) in voices.iter_mut().enumerate() {
                let shift = (voice_index * 2) as u8;
                if beat_header & (BEAT_HAS_VOICE << shift) == 0 {
                    continue;
                }
                if beat_header & (BEAT_HAS_VOICE_CHANGES << shift) != 0 {
                    voice.flags = self.read_u8()?;
                }
                if voice.flags & VOICE_NEXT_DURATION != 0 {
                    voice.duration = self.read_duration()?;
                }
                if voice.flags & VOICE_HAS_NOTES != 0 {
                    let mut note_header = NOTE_HAS_NEXT;
                    while note_header & NOTE_HAS_NEXT != 0 {
                        note_header = self.read_u8()?;
                        let fret = self.read_u8()?;
                        let string_number = self.read_u8()?.saturating_sub(1) as usize;
                        let tied = note_header & NOTE_TIED != 0;
                        if note_header & NOTE_VELOCITY != 0 {
                            voice.velocity = self.read_u8()?;
                        }
                        let effects = if note_header & NOTE_EFFECT != 0 {
                            self.read_note_effects()?
                        } else {
                            NoteEffects::default()
                        };
                        notes.push(TabNote {
                            id: *next_note_id,
                            abs_tick: (beat_start - TG_START_OFFSET).max(0),
                            duration_ticks: voice.duration.ticks,
                            string_index: string_number,
                            fret,
                            velocity: voice.velocity,
                            effects,
                            tied_from_previous: tied.then_some(TieOrigin::User),
                        });
                        *next_note_id += 1;
                    }
                }
                if voice.flags & (VOICE_DIRECTION_UP | VOICE_DIRECTION_DOWN) != 0 {
                    // Direction is encoded in the flags only, no payload.
                }
                voice.start += voice.duration.ticks;
            }
            if beat_header & BEAT_HAS_STROKE != 0 {
                let _direction = self.read_u8()?;
                let _value = self.read_u8()?;
                self.report.skip("strums");
            }
            if beat_header & BEAT_HAS_CHORD != 0 {
                self.report.skip("chord diagrams");
                let strings = self.read_u8()? as usize;
                let _name = self.read_ubyte_string()?;
                let _first_fret = self.read_u8()?;
                for _ in 0..strings {
                    let _ = self.read_u8()?;
                }
            }
            if beat_header & BEAT_HAS_TEXT != 0 {
                let text = self.read_ubyte_string()?;
                texts.push(((beat_start - TG_START_OFFSET).max(0), text));
            }
        }
        if flags & MEASURE_CLEF != 0 {
            let _ = self.read_u8()?;
        }
        if flags & MEASURE_KEYSIGNATURE != 0 {
            let _ = self.read_u8()?;
        }
        Ok(())
    }

    fn read_note_effects(&mut self) -> Result<NoteEffects, FormatError> {
        let raw = self.read_header24()?;
        if raw & EFFECT_BEND != 0 {
            self.skip_points(false)?;
            self.report.skip("bends");
        }
        if raw & EFFECT_TREMOLO_BAR != 0 {
            self.skip_points(false)?;
            self.report.skip("tremolo bar dives");
        }
        if raw & EFFECT_HARMONIC != 0 {
            self.report.skip("harmonics");
            let harmonic_type = self.read_u8()?;
            if harmonic_type != 1 {
                let _ = self.read_u8()?;
            }
        }
        if raw & EFFECT_GRACE != 0 {
            self.report.skip("grace notes (played as ordinary notes)");
            let _flags = self.read_u8()?;
            let _fret = self.read_u8()?;
            let _duration = self.read_u8()?;
            let _dynamic = self.read_u8()?;
            let _transition = self.read_u8()?;
        }
        if raw & EFFECT_TRILL != 0 {
            self.report.skip("trills");
            let _fret = self.read_u8()?;
            let _duration = self.read_u8()?;
        }
        if raw & EFFECT_TREMOLO_PICKING != 0 {
            self.report.skip("tremolo picking");
            let _duration = self.read_u8()?;
        }
        Ok(NoteEffects {
            dead: raw & EFFECT_DEAD != 0,
            palm_mute: raw & EFFECT_PALM_MUTE != 0,
            let_ring: raw & EFFECT_LET_RING != 0,
            vibrato: raw & EFFECT_VIBRATO != 0,
            slide: raw & EFFECT_SLIDE != 0,
            hammer: raw & EFFECT_HAMMER != 0,
            ghost: raw & EFFECT_GHOST != 0,
            accent: raw & EFFECT_ACCENTUATED != 0,
            heavy_accent: raw & EFFECT_HEAVY_ACCENTUATED != 0,
            staccato: raw & EFFECT_STACCATO != 0,
            tapping: raw & EFFECT_TAPPING != 0,
            slapping: raw & EFFECT_SLAPPING != 0,
            popping: raw & EFFECT_POPPING != 0,
            fade_in: raw & EFFECT_FADE_IN != 0,
        })
    }

    fn skip_points(&mut self, signed_value: bool) -> Result<(), FormatError> {
        let count = self.read_u8()? as usize;
        for _ in 0..count {
            let _position = self.read_u8()?;
            let _value = if signed_value {
                self.read_i8()? as i16
            } else {
                self.read_u8()? as i16
            };
        }
        Ok(())
    }

    fn read_marker(&mut self) -> Result<(), FormatError> {
        let _title = self.read_ubyte_string()?;
        let _r = self.read_u8()?;
        let _g = self.read_u8()?;
        let _b = self.read_u8()?;
        Ok(())
    }

    fn read_time_signature(&mut self) -> Result<TimeSignature, FormatError> {
        let numerator = self.read_u8()?.max(1);
        let denominator = self.read_duration()?.value.max(1);
        Ok(TimeSignature::new(numerator, denominator))
    }

    fn read_duration(&mut self) -> Result<TgDuration, FormatError> {
        let flags = self.read_u8()?;
        let value = self.read_u8()?.max(1);
        // The flag is set when the duration is divided in an unusual way:
        // `enters` notes in the time of `times`.
        let (enters, times) = if flags & DURATION_NO_TUPLET != 0 {
            (self.read_u8()?.max(1), self.read_u8()?.max(1))
        } else {
            (1, 1)
        };
        let dots = if flags & DURATION_DOUBLE_DOTTED != 0 {
            2
        } else {
            u8::from(flags & DURATION_DOTTED != 0)
        };
        let ticks = dotted_ticks(value as u32, dots)
            .unwrap_or_else(|| TICKS_PER_QUARTER * 4 / value as Tick);
        Ok(TgDuration {
            value,
            ticks: scaled(ticks, Tick::from(times), Tick::from(enters)),
        })
    }

    fn read_u8(&mut self) -> Result<u8, FormatError> {
        let mut buf = [0];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    fn read_i8(&mut self) -> Result<i8, FormatError> {
        Ok(self.read_u8()? as i8)
    }

    fn read_i16(&mut self) -> Result<i16, FormatError> {
        let mut buf = [0; 2];
        self.cursor.read_exact(&mut buf)?;
        Ok(i16::from_be_bytes(buf))
    }

    fn read_i32(&mut self) -> Result<i32, FormatError> {
        let mut buf = [0; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(i32::from_be_bytes(buf))
    }

    fn read_header24(&mut self) -> Result<u32, FormatError> {
        let b0 = self.read_u8()? as u32;
        let b1 = self.read_u8()? as u32;
        let b2 = self.read_u8()? as u32;
        Ok((b0 << 16) | (b1 << 8) | b2)
    }

    fn read_ubyte_string(&mut self) -> Result<String, FormatError> {
        let len = self.read_u8()? as usize;
        self.read_utf16be_string(len)
    }

    fn read_i32_string(&mut self) -> Result<String, FormatError> {
        let len = self.read_i32()?.max(0) as usize;
        self.read_utf16be_string(len)
    }

    fn read_utf16be_string(&mut self, len: usize) -> Result<String, FormatError> {
        let mut units = Vec::with_capacity(len);
        for _ in 0..len {
            let mut buf = [0; 2];
            self.cursor.read_exact(&mut buf)?;
            units.push(u16::from_be_bytes(buf));
        }
        Ok(String::from_utf16_lossy(&units))
    }
}

#[derive(Default)]
struct TgWriter {
    bytes: Vec<u8>,
}

impl TgWriter {
    fn into_inner(self) -> Vec<u8> {
        self.bytes
    }

    fn write_measure(&mut self, exported: &score::ExportTrack, bar: &ExportBar, bar_start: Tick, first: bool) {
        let mut measure_flags = 0;
        if first {
            measure_flags |= MEASURE_CLEF | MEASURE_KEYSIGNATURE;
        }
        self.write_u8(measure_flags);

        let mut voice = VoiceData::new(bar_start + TG_START_OFFSET);
        for (index, beat) in bar.beats.iter().enumerate() {
            let mut beat_flags = if index + 1 < bar.beats.len() {
                BEAT_HAS_NEXT
            } else {
                0
            };
            beat_flags |= BEAT_HAS_VOICE | BEAT_HAS_VOICE_CHANGES;
            let texts = exported.texts_in(beat);
            if !texts.is_empty() {
                beat_flags |= BEAT_HAS_TEXT;
            }
            let mut voice_flags = if beat.notes.is_empty() {
                0
            } else {
                VOICE_HAS_NOTES
            };
            if beat.duration != voice.duration.ticks {
                voice_flags |= VOICE_NEXT_DURATION;
            }
            self.write_u8(beat_flags);
            self.write_u8(voice_flags);
            if voice_flags & VOICE_NEXT_DURATION != 0 {
                self.write_duration(beat.duration);
                voice.duration = duration_from_ticks(beat.duration);
            }
            for (note_index, note) in beat.notes.iter().enumerate() {
                let mut note_flags = NOTE_VELOCITY;
                if note_index + 1 < beat.notes.len() {
                    note_flags |= NOTE_HAS_NEXT;
                }
                if has_effects(&note.effects) {
                    note_flags |= NOTE_EFFECT;
                }
                if note.tied {
                    note_flags |= NOTE_TIED;
                }
                self.write_u8(note_flags);
                self.write_u8(note.fret);
                self.write_u8((note.string_index + 1).min(255) as u8);
                self.write_u8(note.velocity.min(127));
                if note_flags & NOTE_EFFECT != 0 {
                    self.write_note_effects(&note.effects);
                }
            }
            if !texts.is_empty() {
                let _ = self.write_ubyte_string(&texts.join(" "));
            }
            voice.start += beat.duration;
        }

        if first {
            self.write_u8(0);
            self.write_u8(0);
        }
    }

    fn write_time_signature(&mut self, signature: TimeSignature) {
        self.write_u8(signature.numerator);
        self.write_duration(TICKS_PER_QUARTER * 4 / signature.denominator.max(1) as Tick);
    }

    fn write_duration(&mut self, ticks: Tick) {
        // A length that is exactly a (dotted) note value is written as that
        // value plus its dot flag; any other length as the value it reaches.
        let (value, dots) = match detect_length(ticks) {
            Some((value, dots)) if value <= 64 => (value as u8, dots),
            _ => (duration_from_ticks(ticks).value, 0),
        };
        self.write_u8(match dots {
            1 => DURATION_DOTTED,
            2 => DURATION_DOUBLE_DOTTED,
            _ => 0,
        });
        self.write_u8(value);
    }

    fn write_note_effects(&mut self, effects: &NoteEffects) {
        let mut raw = 0;
        if effects.vibrato {
            raw |= EFFECT_VIBRATO;
        }
        if effects.dead {
            raw |= EFFECT_DEAD;
        }
        if effects.slide {
            raw |= EFFECT_SLIDE;
        }
        if effects.hammer {
            raw |= EFFECT_HAMMER;
        }
        if effects.ghost {
            raw |= EFFECT_GHOST;
        }
        if effects.accent {
            raw |= EFFECT_ACCENTUATED;
        }
        if effects.heavy_accent {
            raw |= EFFECT_HEAVY_ACCENTUATED;
        }
        if effects.palm_mute {
            raw |= EFFECT_PALM_MUTE;
        }
        if effects.staccato {
            raw |= EFFECT_STACCATO;
        }
        if effects.tapping {
            raw |= EFFECT_TAPPING;
        }
        if effects.slapping {
            raw |= EFFECT_SLAPPING;
        }
        if effects.popping {
            raw |= EFFECT_POPPING;
        }
        if effects.fade_in {
            raw |= EFFECT_FADE_IN;
        }
        if effects.let_ring {
            raw |= EFFECT_LET_RING;
        }
        self.bytes.push(((raw >> 16) & 0xff) as u8);
        self.bytes.push(((raw >> 8) & 0xff) as u8);
        self.bytes.push((raw & 0xff) as u8);
    }

    fn write_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn write_i16(&mut self, value: i16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn write_i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn write_ubyte_string(&mut self, value: &str) -> Result<(), FormatError> {
        let units: Vec<u16> = value.encode_utf16().take(255).collect();
        self.write_u8(units.len() as u8);
        self.write_utf16_units(&units);
        Ok(())
    }

    fn write_i32_string(&mut self, value: &str) -> Result<(), FormatError> {
        let units: Vec<u16> = value.encode_utf16().collect();
        self.write_i32(units.len().min(i32::MAX as usize) as i32);
        self.write_utf16_units(&units);
        Ok(())
    }

    fn write_utf16_units(&mut self, units: &[u16]) {
        for unit in units {
            self.bytes.extend_from_slice(&unit.to_be_bytes());
        }
    }
}

/// Where the next beat starts, as TuxGuitar's `TGBeatData.getCurrentStart`
/// decides it: the earliest voice position that lies past the previous beat.
/// A voice that has no beats yet stays put and so never holds the others back.
fn next_beat_start(voices: &[VoiceData; 2], current: &mut Tick) -> Tick {
    let next = voices
        .iter()
        .map(|voice| voice.start)
        .filter(|start| *start > *current)
        .min();
    if let Some(next) = next {
        *current = next;
    }
    *current
}

fn duration_from_ticks(ticks: Tick) -> TgDuration {
    let value = match ticks {
        t if t >= TICKS_PER_QUARTER * 4 => 1,
        t if t >= TICKS_PER_QUARTER * 2 => 2,
        t if t >= TICKS_PER_QUARTER => 4,
        t if t >= TICKS_PER_QUARTER / 2 => 8,
        t if t >= TICKS_PER_QUARTER / 4 => 16,
        t if t >= TICKS_PER_QUARTER / 8 => 32,
        _ => 64,
    };
    TgDuration { value, ticks }
}

fn has_effects(effects: &NoteEffects) -> bool {
    effects.dead
        || effects.palm_mute
        || effects.let_ring
        || effects.vibrato
        || effects.slide
        || effects.hammer
        || effects.ghost
        || effects.accent
        || effects.heavy_accent
        || effects.staccato
        || effects.tapping
        || effects.slapping
        || effects.popping
        || effects.fade_in
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::StringedVariant;

    #[test]
    fn the_header_is_the_text_tuxguitar_detects_the_format_by() {
        // TuxGuitar 1.6.6 reads a length byte and then UTF-16 chars and
        // compares them with "TuxGuitar File Format - 1.5" exactly; any
        // other text and it moves on to its other readers, ending in a zip
        // error.
        let bytes = write_tg(&score::aligned_demo()).unwrap();
        let length = bytes[0] as usize;
        let units: Vec<u16> = bytes[1..1 + length * 2]
            .chunks(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(String::from_utf16(&units).unwrap(), "TuxGuitar File Format - 1.5");
    }

    #[test]
    fn tg_subset_round_trips_betterwriter_project() {
        let project = score::aligned_demo();
        let bytes = write_tg(&project).unwrap();
        let decoded = read_tg(&bytes).unwrap();
        assert_eq!(decoded.tracks.len(), project.tracks.len());
        assert!(!decoded.tracks[0].notes.is_empty());
    }

    #[test]
    fn tg_round_trip_keeps_repeat_signs() {
        let mut project = score::aligned_demo();
        let bars = project.tracks[0].bar_spans();
        for track in &mut project.tracks {
            track.set_repeat_open(bars[0].start, true);
            track.set_repeat_close(bars[1].start, 2);
        }
        let decoded = read_tg(&write_tg(&project).unwrap()).unwrap();
        let marks = &decoded.tracks[0].bar_marks;
        assert_eq!(marks.len(), 2);
        assert!(marks[0].repeat_open && marks[0].repeat_close == 0);
        assert_eq!((marks[1].at_tick, marks[1].repeat_close), (bars[1].start, 2));
    }

    #[test]
    fn tg_writes_a_note_across_a_barline_as_tied_beats() {
        let q = TICKS_PER_QUARTER;
        let mut project = score::aligned_demo();
        project.tracks.truncate(1);
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        project.insert_note(id, q * 3, q * 2, 0, 5).unwrap();
        let decoded = read_tg(&write_tg(&project).unwrap()).unwrap();
        let notes = &decoded.tracks[0].notes;
        assert_eq!(notes.len(), 2);
        assert_eq!((notes[0].abs_tick, notes[0].duration_ticks), (q * 3, q));
        assert_eq!(notes[1].abs_tick, q * 4);
        assert!(notes[1].tied_from_previous.is_some());
    }

    #[test]
    fn tg_round_trip_keeps_sound_volume_and_pan() {
        let mut project = score::aligned_demo();
        project.tracks[0].program = 29;
        project.tracks[0].volume = 80;
        project.tracks[0].pan = -20;
        project.tracks[1].program = 35;
        project.tracks[1].pan = 63;
        let decoded = read_tg(&write_tg(&project).unwrap()).unwrap();

        let first = &decoded.tracks[0];
        assert_eq!((first.program, first.volume, first.pan), (29, 80, -20));
        assert_eq!(first.stringed_variant, StringedVariant::ElectricGuitar);
        let second = &decoded.tracks[1];
        assert_eq!((second.program, second.pan), (35, 63));
        assert_eq!(second.stringed_variant, StringedVariant::Bass);
    }

    #[test]
    fn tg_round_trip_keeps_dotted_and_double_dotted_lengths() {
        let q = TICKS_PER_QUARTER;
        let mut project = score::aligned_demo();
        let track_id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        // One whole bar: dotted quarter, eighth, double-dotted quarter,
        // sixteenth.
        let lengths = [q * 3 / 2, q / 2, q * 7 / 4, q / 4];
        let mut tick = 0;
        for length in lengths {
            project.insert_note(track_id, tick, length, 0, 3).unwrap();
            tick += length;
        }
        let decoded = read_tg(&write_tg(&project).unwrap()).unwrap();
        let read: Vec<Tick> = decoded.tracks[0]
            .notes
            .iter()
            .map(|note| note.duration_ticks)
            .collect();
        assert_eq!(read, lengths);
    }

    #[test]
    fn a_tuplet_duration_is_scaled_to_the_time_it_takes() {
        // An eighth of a triplet: 3 in the time of 2.
        let bytes = [DURATION_NO_TUPLET, 8, 3, 2];
        let duration = TgReader::new(&bytes).read_duration().unwrap();
        assert_eq!(duration.ticks, 320);
        let plain = TgReader::new(&[0, 8]).read_duration().unwrap();
        assert_eq!(plain.ticks, 480);
    }

    #[test]
    fn the_tuning_comes_back_as_written_with_no_offset() {
        let project = score::aligned_demo();
        let back = read_tg(&write_tg(&project).unwrap()).unwrap();
        for (a, b) in project.tracks.iter().zip(&back.tracks) {
            assert_eq!(a.tuning, b.tuning);
        }
    }

    #[test]
    fn our_own_files_import_with_nothing_reported() {
        let import = import_tg(&write_tg(&score::aligned_demo()).unwrap()).unwrap();
        assert!(import.report.is_empty());
    }

    #[test]
    fn imported_tracks_skip_the_drum_channel() {
        let mut project = score::aligned_demo();
        let template = project.tracks[0].clone();
        for index in 3..=11 {
            let mut track = template.clone();
            track.id = index;
            project.tracks.push(track);
        }
        let decoded = read_tg(&write_tg(&project).unwrap()).unwrap();
        assert_eq!(decoded.tracks.len(), 11);
        assert!(decoded.tracks.iter().all(|track| track.channel != 9));
    }
}
