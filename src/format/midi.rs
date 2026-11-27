//! Standard MIDI File export: one conductor track with the tempo and time
//! signature changes, then one track per instrument on its own channel.

use crate::core::{
    BwxProject, TICKS_PER_QUARTER, Tick,
    project::{InstrumentTrack, TabNote, TimeSignature},
    smf::push_variable_length,
    sounds::midi_pan,
};

use super::{
    FormatError,
    score::{self, ExportSong, TempoEvent},
};

/// A message with its place in the file. At one tick, controllers and
/// program changes come first, then note offs, so a repeated pitch
/// retriggers.
struct Event {
    tick: Tick,
    order: u8,
    data: Vec<u8>,
}

fn meta(tick: Tick, kind: u8, payload: &[u8]) -> Event {
    let mut data = vec![0xff, kind];
    push_variable_length(&mut data, payload.len() as u32);
    data.extend_from_slice(payload);
    Event {
        tick,
        order: 0,
        data,
    }
}

fn channel_message(tick: Tick, order: u8, status: u8, first: u8, second: Option<u8>) -> Event {
    let mut data = vec![status, first];
    data.extend(second);
    Event { tick, order, data }
}

fn track_chunk(mut events: Vec<Event>, end: Tick) -> Vec<u8> {
    events.sort_by_key(|event| (event.tick, event.order));
    let mut body = Vec::new();
    let mut last = 0;
    for event in &events {
        push_variable_length(&mut body, (event.tick - last).max(0) as u32);
        last = last.max(event.tick);
        body.extend_from_slice(&event.data);
    }
    push_variable_length(&mut body, (end - last).max(0) as u32);
    body.extend([0xff, 0x2f, 0x00]);
    let mut chunk = b"MTrk".to_vec();
    chunk.extend((body.len() as u32).to_be_bytes());
    chunk.extend(body);
    chunk
}

/// The tempo that plays from event `index` on: a gliding point is played at
/// the average of its own and the next point's tempo, as the player does.
fn playing_bpm(tempos: &[TempoEvent], index: usize) -> f32 {
    let event = tempos[index];
    match tempos.get(index + 1) {
        Some(next) if event.linear => (event.bpm + next.bpm) / 2.0,
        _ => event.bpm,
    }
}

fn tempo_message(tick: Tick, bpm: f32) -> Event {
    let microseconds = (60_000_000.0 / f64::from(bpm.max(1.0))).round() as u32;
    meta(tick, 0x51, &microseconds.to_be_bytes()[1..])
}

fn signature_message(tick: Tick, signature: TimeSignature) -> Event {
    let power = signature.denominator.max(1).ilog2() as u8;
    meta(tick, 0x58, &[signature.numerator, power, 24, 8])
}

/// The conductor track's events and the length of the played song.
fn conductor(song: &ExportSong, title: &str) -> (Vec<Event>, Tick) {
    let first = &song.project.tracks[0];
    let spans = first.bar_spans();
    let mut events = vec![meta(0, 0x03, title.as_bytes())];
    let mut last_signature = None;
    let mut last_bpm: Option<f32> = None;
    let mut end = 0;
    for played in first.played_bars() {
        let span = spans[played.bar];
        end = played.end;
        if last_signature != Some(span.signature) {
            events.push(signature_message(played.start, span.signature));
            last_signature = Some(span.signature);
        }
        let mut changes: Vec<(Tick, f32)> = Vec::new();
        let in_effect = song
            .tempos
            .iter()
            .rposition(|event| event.tick <= span.start)
            .unwrap_or(0);
        changes.push((played.start, playing_bpm(&song.tempos, in_effect)));
        for (index, event) in song.tempos.iter().enumerate() {
            if event.tick > span.start && event.tick < span.end {
                changes.push((
                    played.start + (event.tick - span.start),
                    playing_bpm(&song.tempos, index),
                ));
            }
        }
        for (tick, bpm) in changes {
            if last_bpm != Some(bpm) {
                events.push(tempo_message(tick, bpm));
                last_bpm = Some(bpm);
            }
        }
    }
    (events, end)
}

/// Sounding notes of a track as (start, end, key, velocity), with tied notes
/// joined into one.
fn sounding_notes(track: &InstrumentTrack) -> Vec<(Tick, Tick, u8, u8)> {
    let mut sorted: Vec<&TabNote> = track.notes.iter().collect();
    sorted.sort_by_key(|note| (note.abs_tick, note.string_index));
    let mut out: Vec<(Tick, Tick, u8, u8)> = Vec::new();
    let mut open: Vec<(usize, usize)> = Vec::new();
    for note in sorted {
        let held = track
            .is_tied_continuation(note)
            .then(|| {
                open.iter()
                    .find(|(string, index)| *string == note.string_index && out[*index].1 == note.abs_tick)
                    .map(|(_, index)| *index)
            })
            .flatten();
        if let Some(index) = held {
            out[index].1 = note.end_tick();
            continue;
        }
        open.retain(|(string, _)| *string != note.string_index);
        open.push((note.string_index, out.len()));
        out.push((
            note.abs_tick,
            note.end_tick().max(note.abs_tick + 1),
            track.midi_key_for(note),
            note.velocity.clamp(1, 127),
        ));
    }
    out
}

fn instrument_events(track: &InstrumentTrack) -> Vec<Event> {
    let channel = track.channel.min(15);
    let (bank, program) = track.sound();
    let mut events = vec![meta(0, 0x03, track.name.as_bytes())];
    if bank != 0 && bank < 128 {
        events.push(channel_message(0, 0, 0xb0 | channel, 0, Some((bank >> 7) as u8 & 0x7f)));
        events.push(channel_message(0, 0, 0xb0 | channel, 32, Some(bank as u8 & 0x7f)));
    }
    events.push(channel_message(0, 0, 0xb0 | channel, 7, Some(track.volume.min(127))));
    events.push(channel_message(0, 0, 0xb0 | channel, 10, Some(midi_pan(track.pan))));
    events.push(channel_message(0, 0, 0xc0 | channel, program.min(127), None));

    let played = track.unrolled_for_playback();
    let played = played.as_ref().unwrap_or(track);
    for (start, stop, key, velocity) in sounding_notes(played) {
        events.push(channel_message(start, 2, 0x90 | channel, key, Some(velocity)));
        events.push(channel_message(stop, 1, 0x80 | channel, key, Some(0)));
    }
    events.extend(pitch_bend_events(played, channel));
    events
}

/// A pitch wheel message: `offset` from the wheel's centre.
fn pitch_wheel(tick: Tick, order: u8, channel: u8, offset: i16) -> Event {
    let value = (8192 + i32::from(offset)).clamp(0, 16383) as u16;
    channel_message(tick, order, 0xe0 | channel, (value & 0x7f) as u8, Some((value >> 7) as u8))
}

/// The bends of a track's notes and the dives of its tremolo bar as pitch
/// wheel messages, after a registered parameter message that sets the wheel's
/// range. The wheel bends every note of the channel, so notes that sound
/// together move together; that is all a MIDI file can say.
fn pitch_bend_events(track: &InstrumentTrack, channel: u8) -> Vec<Event> {
    let mut events = Vec::new();
    let bent: Vec<_> = track
        .notes
        .iter()
        .filter(|note| !track.is_tied_continuation(note))
        .filter_map(|note| {
            let bend = note.effects.bend.as_ref().filter(|bend| !bend.is_flat())?;
            Some((note, bend))
        })
        .collect();
    if bent.is_empty() && track.tremolo_bar.is_empty() {
        return events;
    }
    for (controller, value) in [
        (101, 0),
        (100, 0),
        (6, crate::core::pitch::WHEEL_RANGE_SEMITONES),
        (38, 0),
        (101, 127),
        (100, 127),
    ] {
        events.push(channel_message(0, 0, 0xb0 | channel, controller, Some(value)));
    }
    for (note, bend) in bent {
        let end = track.sounding_end(note);
        for (offset, wheel) in bend.wheel_steps(0, end - note.abs_tick) {
            if note.abs_tick + offset < end {
                events.push(pitch_wheel(note.abs_tick + offset, 0, channel, wheel));
            }
        }
        let resting = crate::core::pitch::wheel_offset(track.tremolo_at(end));
        events.push(pitch_wheel(end, 1, channel, resting));
    }
    for dive in &track.tremolo_bar {
        for (offset, wheel) in dive.envelope.wheel_steps(0, dive.envelope.length()) {
            events.push(pitch_wheel(dive.start + offset, 0, channel, wheel));
        }
    }
    events
}

pub fn write_midi(project: &BwxProject) -> Result<Vec<u8>, FormatError> {
    let song = score::prepare(project)?;
    let (conductor_events, end) = conductor(&song, &project.title);
    let mut file = b"MThd".to_vec();
    file.extend(6_u32.to_be_bytes());
    file.extend(1_u16.to_be_bytes());
    file.extend(((project.tracks.len() + 1).min(u16::MAX as usize) as u16).to_be_bytes());
    file.extend((TICKS_PER_QUARTER as u16).to_be_bytes());
    file.extend(track_chunk(conductor_events, end));
    for track in &project.tracks {
        file.extend(track_chunk(instrument_events(track), end));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{TempoPoint, smf};

    const Q: Tick = TICKS_PER_QUARTER;

    fn single_track(notes: &[(Tick, Tick, usize, u8)]) -> BwxProject {
        let mut project = BwxProject::demo();
        project.tracks.truncate(1);
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        for &(start, length, string, fret) in notes {
            project.insert_note(id, start, length, string, fret).unwrap();
        }
        project
    }

    fn find(file: &[u8], pattern: &[u8]) -> Option<usize> {
        file.windows(pattern.len()).position(|window| window == pattern)
    }

    #[test]
    fn the_file_reads_back_with_the_notes_in_place() {
        let project = single_track(&[(0, Q, 5, 0), (Q, Q / 2, 4, 2), (Q * 2, Q * 2, 3, 2)]);
        let file = write_midi(&project).unwrap();
        assert_eq!(&file[..4], b"MThd");
        let notes = smf::read_notes(&file).unwrap();
        let tuning = &project.tracks[0].tuning;
        let expected = [
            (0, Q, tuning[5]),
            (Q, Q / 2, tuning[4] + 2),
            (Q * 2, Q * 2, tuning[3] + 2),
        ];
        let read: Vec<(Tick, Tick, u8)> = notes
            .iter()
            .map(|note| (note.start, note.duration, note.key))
            .collect();
        assert_eq!(read, expected);
    }

    #[test]
    fn the_header_counts_a_conductor_and_every_track() {
        let project = score::aligned_demo();
        let file = write_midi(&project).unwrap();
        assert_eq!(u16::from_be_bytes([file[8], file[9]]), 1);
        assert_eq!(
            usize::from(u16::from_be_bytes([file[10], file[11]])),
            project.tracks.len() + 1
        );
        assert_eq!(u16::from_be_bytes([file[12], file[13]]), TICKS_PER_QUARTER as u16);
    }

    #[test]
    fn tempo_and_signature_changes_sit_at_their_bars() {
        let mut project = single_track(&[(0, Q, 5, 0)]);
        let bar_two = project.tracks[0].bar_spans()[1].start;
        project.tempo_points = vec![TempoPoint::new(0, 120.0), TempoPoint::new(bar_two, 90.0)];
        let file = write_midi(&project).unwrap();
        assert!(find(&file, &[0xff, 0x51, 0x03, 0x07, 0xa1, 0x20]).is_some());
        assert!(find(&file, &[0xff, 0x51, 0x03, 0x0a, 0x2c, 0x2b]).is_some());
        assert!(find(&file, &[0xff, 0x58, 0x04, 4, 2, 24, 8]).is_some());
    }

    #[test]
    fn a_gliding_tempo_plays_at_the_average_of_its_ends() {
        let mut project = single_track(&[(0, Q, 5, 0)]);
        let bar_two = project.tracks[0].bar_spans()[1].start;
        let mut start = TempoPoint::new(0, 100.0);
        start.transition = crate::core::project::TempoTransition::Progressive;
        project.tempo_points = vec![start, TempoPoint::new(bar_two, 140.0)];
        let file = write_midi(&project).unwrap();
        // 120 bpm is 500000 microseconds per quarter.
        assert!(find(&file, &[0xff, 0x51, 0x03, 0x07, 0xa1, 0x20]).is_some());
    }

    #[test]
    fn tied_notes_sound_as_one() {
        let mut project = single_track(&[(0, Q, 5, 3), (Q, Q, 5, 3)]);
        let id = project.tracks[0].id;
        let second = project.tracks[0].notes[1].id;
        project.track_mut(id).unwrap().notes.iter_mut().find(|n| n.id == second).unwrap().tied_from_previous =
            Some(crate::core::project::TieOrigin::User);
        let notes = smf::read_notes(&write_midi(&project).unwrap()).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].duration, Q * 2);
    }

    #[test]
    fn repeated_bars_are_played_out() {
        let mut project = single_track(&[(0, Q, 5, 0)]);
        project.tracks[0].set_repeat_close(0, 1);
        let notes = smf::read_notes(&write_midi(&project).unwrap()).unwrap();
        let bar = project.tracks[0].bar_spans()[0].end;
        assert_eq!(notes.iter().map(|note| note.start).collect::<Vec<_>>(), [0, bar]);
    }

    #[test]
    fn each_track_gets_its_channel_sound_volume_and_pan() {
        let mut project = score::aligned_demo();
        project.tracks[1].channel = 3;
        project.tracks[1].volume = 70;
        project.tracks[1].pan = -20;
        project.tracks[1].sound_auto = false;
        project.tracks[1].bank = 0;
        project.tracks[1].program = 33;
        let file = write_midi(&project).unwrap();
        assert!(find(&file, &[0xb3, 7, 70]).is_some());
        assert!(find(&file, &[0xb3, 10, 44]).is_some());
        assert!(find(&file, &[0xc3, 33]).is_some());
    }

    #[test]
    fn an_incompatible_project_is_refused() {
        let mut project = BwxProject::demo();
        project.tracks[1].time_signature_changes = vec![crate::core::project::TimeSignatureChange {
            at_tick: 0,
            signature: TimeSignature::new(3, 4),
        }];
        assert!(write_midi(&project).is_err());
    }
}
