use std::io::{Cursor, Read};

use crate::core::{
    BwxProject, NoteEffects, TICKS_PER_QUARTER, Tick,
    project::{InstrumentTrack, TabNote, TimeSignature, TimeSignatureChange},
};

use super::FormatError;

const TG_VERSION: &str = "TuxGuitar File Format - 1.5.0";
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
}

pub fn read_tg(bytes: &[u8]) -> Result<BwxProject, FormatError> {
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
    let mut channel_names = Vec::with_capacity(channel_count);
    for _ in 0..channel_count {
        let _channel_id = reader.read_i16()? as u16;
        let _bank = reader.read_u8()?;
        let program = reader.read_u8()?;
        let _volume = reader.read_u8()?;
        let _balance = reader.read_u8()?;
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
        let _repeat_open = flags & MEASURE_HEADER_REPEAT_OPEN != 0;
        if flags & MEASURE_HEADER_REPEAT_CLOSE != 0 {
            let _ = reader.read_i16()?;
        }
        if flags & MEASURE_HEADER_REPEAT_ALTERNATIVE != 0 {
            let _ = reader.read_u8()?;
        }
        if flags & MEASURE_HEADER_MARKER != 0 {
            let _ = reader.read_marker()?;
        }
        if flags & MEASURE_HEADER_TRIPLET_FEEL != 0 {
            let _ = reader.read_u8()?;
        }
        let length = last_signature.measure_ticks();
        headers.push(MeasureHeader {
            start,
            signature: last_signature,
            tempo: last_tempo,
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
        let program = channel_programs
            .get(channel_id.saturating_sub(1))
            .copied()
            .unwrap_or(24);
        let mut notes = Vec::new();

        for header in &headers {
            reader.read_measure(header, &mut notes, &mut next_note_id)?;
        }

        let string_count = reader.read_u8()? as usize;
        let mut tuning = Vec::with_capacity(string_count);
        for _ in 0..string_count {
            tuning.push(reader.read_u8()?);
        }
        let _offset = reader.read_u8()?;
        let _r = reader.read_u8()?;
        let _g = reader.read_u8()?;
        let _b = reader.read_u8()?;
        if track_flags & TRACK_LYRICS != 0 {
            let _from = reader.read_i16()?;
            let _lyrics = reader.read_i32_string()?;
        }

        tracks.push(InstrumentTrack {
            id: (track_index + 1) as u64,
            name: if name.is_empty() {
                channel_names
                    .get(track_index)
                    .cloned()
                    .unwrap_or_else(|| format!("Track {}", track_index + 1))
            } else {
                name
            },
            channel: track_index.min(15) as u8,
            program,
            solo: track_flags & TRACK_SOLO != 0,
            mute: track_flags & TRACK_MUTE != 0,
            volume: 100,
            pan: 0,
            tuning: if tuning.is_empty() {
                vec![64, 59, 55, 50, 45, 40]
            } else {
                tuning
            },
            fret_count: 24,
            time_signature_changes: signature_changes(&headers),
            notes,
        });
    }

    Ok(BwxProject {
        schema_version: 1,
        title: if title.is_empty() {
            "Imported TuxGuitar score".to_owned()
        } else {
            title
        },
        tempo_bpm: headers.first().map(|h| h.tempo as f32).unwrap_or(120.0),
        tracks,
        next_note_id,
    })
}

pub fn write_tg(project: &BwxProject) -> Result<Vec<u8>, FormatError> {
    let mut writer = TgWriter::default();
    writer.write_ubyte_string(TG_VERSION)?;
    writer.write_ubyte_string(&project.title)?;
    for value in ["", "", "", "", "", "BetterWriter", ""] {
        writer.write_ubyte_string(value)?;
    }
    writer.write_i32_string("Generated by BetterWriter")?;

    writer.write_u8(project.tracks.len().min(255) as u8);
    for (index, track) in project.tracks.iter().enumerate() {
        writer.write_i16((index + 1).min(i16::MAX as usize) as i16);
        writer.write_u8(0);
        writer.write_u8(track.program.min(127));
        writer.write_u8(track.volume.min(127));
        writer.write_u8((64_i16 + track.pan as i16).clamp(0, 127) as u8);
        writer.write_u8(0);
        writer.write_u8(24);
        writer.write_u8(0);
        writer.write_u8(0);
        writer.write_ubyte_string(&track.name)?;
        writer.write_i16(0);
    }

    let headers = build_headers(project);
    writer.write_i16(headers.len().min(i16::MAX as usize) as i16);
    let mut last_signature = None;
    let mut last_tempo = None;
    for header in &headers {
        let mut flags = 0;
        if last_signature != Some(header.signature) {
            flags |= MEASURE_HEADER_TIMESIGNATURE;
        }
        if last_tempo != Some(header.tempo) {
            flags |= MEASURE_HEADER_TEMPO;
        }
        writer.write_u8(flags);
        if flags & MEASURE_HEADER_TIMESIGNATURE != 0 {
            writer.write_time_signature(header.signature);
        }
        if flags & MEASURE_HEADER_TEMPO != 0 {
            writer.write_i16(header.tempo as i16);
        }
        last_signature = Some(header.signature);
        last_tempo = Some(header.tempo);
    }

    writer.write_u8(project.tracks.len().min(255) as u8);
    for (track_index, track) in project.tracks.iter().enumerate() {
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
        for (measure_index, header) in headers.iter().enumerate() {
            writer.write_measure(track, header, measure_index == 0)?;
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

fn build_headers(project: &BwxProject) -> Vec<MeasureHeader> {
    let base_track = project.tracks.first();
    let mut cursor = TG_START_OFFSET;
    let horizon = project
        .tracks
        .iter()
        .flat_map(|track| track.notes.iter().map(|note| note.end_tick()))
        .max()
        .unwrap_or(TICKS_PER_QUARTER * 4)
        + TICKS_PER_QUARTER * 4;
    let mut headers = Vec::new();
    while cursor - TG_START_OFFSET <= horizon.max(TICKS_PER_QUARTER * 4) {
        let local_tick = cursor - TG_START_OFFSET;
        let signature = base_track
            .map(|track| track.signature_at(local_tick))
            .unwrap_or(TimeSignature::new(4, 4));
        headers.push(MeasureHeader {
            start: cursor,
            signature,
            tempo: project.tempo_bpm.round().clamp(1.0, i16::MAX as f32) as u16,
        });
        cursor += signature.measure_ticks().max(TICKS_PER_QUARTER / 4);
    }
    headers
}

struct TgReader<'a> {
    cursor: Cursor<&'a [u8]>,
}

impl<'a> TgReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(bytes),
        }
    }

    fn read_measure(
        &mut self,
        header: &MeasureHeader,
        notes: &mut Vec<TabNote>,
        next_note_id: &mut u64,
    ) -> Result<(), FormatError> {
        let flags = self.read_u8()?;
        let mut voices = [VoiceData::new(header.start), VoiceData::new(header.start)];
        let mut beat_header = BEAT_HAS_NEXT;
        while beat_header & BEAT_HAS_NEXT != 0 {
            beat_header = self.read_u8()?;
            let beat_start = current_start(&mut voices);
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
                        let _tied = note_header & NOTE_TIED != 0;
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
            }
            if beat_header & BEAT_HAS_CHORD != 0 {
                let strings = self.read_u8()? as usize;
                let _name = self.read_ubyte_string()?;
                let _first_fret = self.read_u8()?;
                for _ in 0..strings {
                    let _ = self.read_u8()?;
                }
            }
            if beat_header & BEAT_HAS_TEXT != 0 {
                let _ = self.read_ubyte_string()?;
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
        }
        if raw & EFFECT_TREMOLO_BAR != 0 {
            self.skip_points(false)?;
        }
        if raw & EFFECT_HARMONIC != 0 {
            let harmonic_type = self.read_u8()?;
            if harmonic_type != 1 {
                let _ = self.read_u8()?;
            }
        }
        if raw & EFFECT_GRACE != 0 {
            let _flags = self.read_u8()?;
            let _fret = self.read_u8()?;
            let _duration = self.read_u8()?;
            let _dynamic = self.read_u8()?;
            let _transition = self.read_u8()?;
        }
        if raw & EFFECT_TRILL != 0 {
            let _fret = self.read_u8()?;
            let _duration = self.read_u8()?;
        }
        if raw & EFFECT_TREMOLO_PICKING != 0 {
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
        if flags & DURATION_NO_TUPLET != 0 {
            let _enters = self.read_u8()?;
            let _times = self.read_u8()?;
        }
        let mut ticks = TICKS_PER_QUARTER * 4 / value as Tick;
        if flags & DURATION_DOTTED != 0 {
            ticks += ticks / 2;
        }
        if flags & DURATION_DOUBLE_DOTTED != 0 {
            ticks += ticks / 4;
        }
        Ok(TgDuration { value, ticks })
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

    fn write_measure(
        &mut self,
        track: &InstrumentTrack,
        header: &MeasureHeader,
        first: bool,
    ) -> Result<(), FormatError> {
        let measure_start = header.start - TG_START_OFFSET;
        let measure_end = measure_start + header.signature.measure_ticks();
        let mut note_items: Vec<BeatItem> = track
            .notes
            .iter()
            .filter(|note| note.abs_tick >= measure_start && note.abs_tick < measure_end)
            .cloned()
            .map(BeatItem::note)
            .collect();
        note_items.sort_by_key(|item| (item.start, item.duration));
        let mut items: Vec<BeatItem> = Vec::new();
        for item in note_items {
            if let Some(last) = items.last_mut()
                && last.start == item.start
                && last.duration == item.duration
            {
                last.notes.extend(item.notes);
                continue;
            }
            items.push(item);
        }

        let mut beats = Vec::new();
        let mut cursor = measure_start;
        for item in items {
            if item.start > cursor {
                beats.push(BeatItem::rest(cursor, item.start - cursor));
            }
            cursor = cursor.max(item.start + item.duration);
            beats.push(item);
        }
        if beats.is_empty() {
            beats.push(BeatItem::rest(
                measure_start,
                header.signature.measure_ticks(),
            ));
        }

        let mut measure_flags = 0;
        if first {
            measure_flags |= MEASURE_CLEF | MEASURE_KEYSIGNATURE;
        }
        self.write_u8(measure_flags);

        let mut voice = VoiceData::new(header.start);
        for (index, beat) in beats.iter().enumerate() {
            let mut beat_flags = if index + 1 < beats.len() {
                BEAT_HAS_NEXT
            } else {
                0
            };
            beat_flags |= BEAT_HAS_VOICE | BEAT_HAS_VOICE_CHANGES;
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
            if voice_flags & VOICE_HAS_NOTES != 0 {
                for (note_index, note) in beat.notes.iter().enumerate() {
                    let mut note_flags = NOTE_VELOCITY;
                    if note_index + 1 < beat.notes.len() {
                        note_flags |= NOTE_HAS_NEXT;
                    }
                    if has_effects(&note.effects) {
                        note_flags |= NOTE_EFFECT;
                    }
                    self.write_u8(note_flags);
                    self.write_u8(note.fret);
                    self.write_u8((note.string_index + 1).min(255) as u8);
                    self.write_u8(note.velocity.min(127));
                    if note_flags & NOTE_EFFECT != 0 {
                        self.write_note_effects(&note.effects);
                    }
                }
            }
            voice.start += beat.duration;
        }

        if first {
            self.write_u8(0);
            self.write_u8(0);
        }
        Ok(())
    }

    fn write_time_signature(&mut self, signature: TimeSignature) {
        self.write_u8(signature.numerator);
        self.write_duration(TICKS_PER_QUARTER * 4 / signature.denominator.max(1) as Tick);
    }

    fn write_duration(&mut self, ticks: Tick) {
        let duration = duration_from_ticks(ticks);
        self.write_u8(0);
        self.write_u8(duration.value);
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

#[derive(Clone)]
struct BeatItem {
    start: Tick,
    duration: Tick,
    notes: Vec<TabNote>,
}

impl BeatItem {
    fn note(note: TabNote) -> Self {
        Self {
            start: note.abs_tick,
            duration: note.duration_ticks,
            notes: vec![note],
        }
    }

    fn rest(start: Tick, duration: Tick) -> Self {
        Self {
            start,
            duration,
            notes: Vec::new(),
        }
    }
}

fn current_start(voices: &mut [VoiceData; 2]) -> Tick {
    let current = voices.iter().map(|voice| voice.start).min().unwrap_or(0);
    for voice in voices {
        if voice.start < current {
            voice.start = current;
        }
    }
    current
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

    #[test]
    fn tg_subset_round_trips_betterwriter_project() {
        let project = BwxProject::demo();
        let bytes = write_tg(&project).unwrap();
        let decoded = read_tg(&bytes).unwrap();
        assert_eq!(decoded.tracks.len(), project.tracks.len());
        assert!(!decoded.tracks[0].notes.is_empty());
    }
}
