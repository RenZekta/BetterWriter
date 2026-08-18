use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use super::project::{BwxProject, Tick};

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
}

#[derive(Clone, Default)]
pub struct ShadowTimeline {
    events: Arc<RwLock<Vec<MidiPlaybackEvent>>>,
}

impl ShadowTimeline {
    pub fn compile_from_project(project: &BwxProject) -> Vec<MidiPlaybackEvent> {
        let mut events = Vec::new();

        for track in &project.tracks {
            for note in &track.notes {
                let key = track.midi_key_for(note);
                events.push(MidiPlaybackEvent {
                    tick: note.abs_tick,
                    kind: MidiEventKind::NoteOn,
                    track_id: track.id,
                    channel: track.channel,
                    key,
                    velocity: note.velocity,
                });
                events.push(MidiPlaybackEvent {
                    tick: note.end_tick(),
                    kind: MidiEventKind::NoteOff,
                    track_id: track.id,
                    channel: track.channel,
                    key,
                    velocity: 0,
                });
            }
        }

        events.sort_by_key(|event| {
            let off_first = match event.kind {
                MidiEventKind::NoteOff => 0,
                MidiEventKind::NoteOn => 1,
            };
            (event.tick, off_first, event.channel, event.key)
        });
        events
    }

    pub fn rebuild_all(&self, project: &BwxProject) {
        let compiled = Self::compile_from_project(project);
        if let Ok(mut guard) = self.events.write() {
            *guard = compiled;
        }
    }

    pub fn replace_track(&self, project: &BwxProject, track_id: u64) {
        let mut compiled_track = Self::compile_from_project(&BwxProject {
            schema_version: project.schema_version,
            title: project.title.clone(),
            tempo_points: project.tempo_points.clone(),
            tracks: project
                .tracks
                .iter()
                .filter(|track| track.id == track_id)
                .cloned()
                .collect(),
            next_note_id: project.next_note_id,
        });

        if let Ok(mut guard) = self.events.write() {
            guard.retain(|event| event.track_id != track_id);
            guard.append(&mut compiled_track);
            guard.sort_by_key(|event| {
                let off_first = match event.kind {
                    MidiEventKind::NoteOff => 0,
                    MidiEventKind::NoteOn => 1,
                };
                (event.tick, off_first, event.channel, event.key)
            });
        }
    }

    pub fn snapshot(&self) -> Vec<MidiPlaybackEvent> {
        self.events
            .read()
            .map(|events| events.clone())
            .unwrap_or_default()
    }
}
