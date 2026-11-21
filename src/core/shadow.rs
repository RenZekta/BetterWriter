use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use super::project::{BwxProject, InstrumentTrack, TabNote, Tick};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MidiEventKind {
    NoteOn,
    NoteOff,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MidiPlaybackEvent {
    pub tick: Tick,
    pub kind: MidiEventKind,
    pub track_id: u64,
    pub channel: u8,
    pub key: u8,
    pub velocity: u8,
    /// Preset the note sounds with when it is not the track's usual one.
    pub sound: Option<(u16, u8)>,
}

#[derive(Clone, Default)]
pub struct ShadowTimeline {
    events: Arc<RwLock<Vec<MidiPlaybackEvent>>>,
}

/// The note events of one track, in playback order of its bars.
fn compile_track(written: &InstrumentTrack, events: &mut Vec<MidiPlaybackEvent>) {
    // Repeats play as extra copies of their bars.
    let unrolled = written.unrolled_for_playback();
    let track = unrolled.as_ref().unwrap_or(written);
    // A tied continuation sustains the previous note instead of striking
    // again: it emits no NoteOn, and the note before it emits no NoteOff, so
    // a chain plays as one long note.
    let continuations: Vec<&TabNote> = track
        .notes
        .iter()
        .filter(|note| track.is_tied_continuation(note))
        .collect();
    let is_continued = |note: &TabNote| {
        continuations.iter().any(|next| {
            next.string_index == note.string_index && next.abs_tick == note.end_tick()
        })
    };
    let is_continuation = |note: &TabNote| continuations.iter().any(|c| c.id == note.id);

    for note in &track.notes {
        let key = track.midi_key_for(note);
        if !is_continuation(note) {
            events.push(MidiPlaybackEvent {
                tick: note.abs_tick,
                kind: MidiEventKind::NoteOn,
                track_id: track.id,
                channel: track.channel,
                key,
                velocity: note.velocity,
                sound: track.note_sound(note),
            });
        }
        if !is_continued(note) {
            events.push(MidiPlaybackEvent {
                tick: note.end_tick(),
                kind: MidiEventKind::NoteOff,
                track_id: track.id,
                channel: track.channel,
                key,
                velocity: 0,
                sound: None,
            });
        }
    }
}

fn sort_events(events: &mut [MidiPlaybackEvent]) {
    events.sort_by_key(|event| {
        let off_first = match event.kind {
            MidiEventKind::NoteOff => 0,
            MidiEventKind::NoteOn => 1,
        };
        (event.tick, off_first, event.channel, event.key)
    });
}

impl ShadowTimeline {
    pub fn compile_from_project(project: &BwxProject) -> Vec<MidiPlaybackEvent> {
        let mut events = Vec::new();
        for track in &project.tracks {
            compile_track(track, &mut events);
        }
        sort_events(&mut events);
        events
    }

    pub fn rebuild_all(&self, project: &BwxProject) {
        let compiled = Self::compile_from_project(project);
        if let Ok(mut guard) = self.events.write() {
            *guard = compiled;
        }
    }

    pub fn replace_track(&self, project: &BwxProject, track_id: u64) {
        let mut compiled_track = Vec::new();
        if let Some(track) = project.track(track_id) {
            compile_track(track, &mut compiled_track);
        }
        if let Ok(mut guard) = self.events.write() {
            guard.retain(|event| event.track_id != track_id);
            guard.append(&mut compiled_track);
            sort_events(&mut guard);
        }
    }

    pub fn snapshot(&self) -> Vec<MidiPlaybackEvent> {
        self.events
            .read()
            .map(|events| events.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TICKS_PER_QUARTER;

    #[test]
    fn a_split_brick_plays_as_one_sustained_note() {
        let mut project = BwxProject::demo();
        let bar = project.tracks[0].measure_bounds_at(0).1;
        let id = project
            .insert_note(1, bar - TICKS_PER_QUARTER, TICKS_PER_QUARTER * 2, 0, 5)
            .unwrap();
        let brick_events = ShadowTimeline::compile_from_project(&project);

        assert!(project.split_bricks_at_barlines());
        let split_events = ShadowTimeline::compile_from_project(&project);

        assert_eq!(brick_events, split_events);
        let key = project.tracks[0].midi_key_for(
            project.tracks[0]
                .notes
                .iter()
                .find(|note| note.id == id)
                .unwrap(),
        );
        let ons = split_events
            .iter()
            .filter(|event| {
                event.key == key && event.kind == MidiEventKind::NoteOn && event.track_id == 1
            })
            .count();
        assert_eq!(ons, 1);
    }

    #[test]
    fn a_user_tie_also_suppresses_the_second_attack() {
        let mut project = BwxProject::demo();
        project.insert_note(1, 0, TICKS_PER_QUARTER, 0, 5);
        let second = project
            .insert_note(1, TICKS_PER_QUARTER, TICKS_PER_QUARTER, 0, 5)
            .unwrap();
        let untied = ShadowTimeline::compile_from_project(&project);
        assert!(project.toggle_user_tie(1, second));
        let tied = ShadowTimeline::compile_from_project(&project);
        assert_eq!(untied.len(), tied.len() + 2);
    }
}
