//! Text written above the score at a place in a track: a cue, a section
//! name, a playing hint. It is only for reading; it does not sound.

use serde::{Deserialize, Serialize};

use super::project::{InstrumentTrack, Tick};

/// Longest text kept, in characters.
pub const MAX_TEXT_CHARS: usize = 120;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextAnnotation {
    pub tick: Tick,
    pub text: String,
}

impl InstrumentTrack {
    /// The text written at exactly `tick`.
    pub fn annotation_at(&self, tick: Tick) -> Option<&TextAnnotation> {
        self.annotations.iter().find(|note| note.tick == tick)
    }

    /// Writes `text` at `tick`, replacing what was there; empty text removes
    /// it. Returns whether anything changed.
    pub fn set_annotation(&mut self, tick: Tick, text: &str) -> bool {
        let text: String = text
            .trim()
            .chars()
            .filter(|ch| !ch.is_control())
            .take(MAX_TEXT_CHARS)
            .collect();
        let tick = tick.max(0);
        let before = self.annotations.clone();
        self.annotations.retain(|note| note.tick != tick);
        if !text.is_empty() {
            self.annotations.push(TextAnnotation { tick, text });
            self.annotations.sort_by_key(|note| note.tick);
        }
        self.annotations != before
    }

    /// Moves the texts at or after `from` by `delta` ticks, with the bars.
    pub fn shift_annotations(&mut self, from: Tick, delta: Tick) {
        for note in &mut self.annotations {
            if note.tick >= from {
                note.tick += delta;
            }
        }
    }

    /// Removes the texts that start in `from..to`.
    pub fn drop_annotations_in(&mut self, from: Tick, to: Tick) {
        self.annotations.retain(|note| note.tick < from || note.tick >= to);
    }
}

#[cfg(test)]
mod tests {
    use crate::core::project::BwxProject;

    #[test]
    fn a_text_replaces_the_one_at_its_place_and_empty_text_removes_it() {
        let mut project = BwxProject::demo();
        let track = &mut project.tracks[0];
        track.annotations.clear();
        assert!(track.set_annotation(960, "  Verse "));
        assert_eq!(track.annotation_at(960).map(|note| note.text.as_str()), Some("Verse"));
        assert!(track.set_annotation(960, "Chorus"));
        assert!(!track.set_annotation(960, "Chorus"));
        assert_eq!(track.annotations.len(), 1);
        assert!(track.set_annotation(0, "Intro"));
        assert_eq!(track.annotations[0].text, "Intro");
        assert!(track.set_annotation(960, ""));
        assert_eq!(track.annotations.len(), 1);
    }

    #[test]
    fn texts_follow_bar_edits() {
        let mut project = BwxProject::demo();
        let id = project.tracks[0].id;
        project.tracks[0].annotations.clear();
        let width = project.tracks[0].measure_bounds_at(0).1;
        project.tracks[0].set_annotation(width + 10, "Bridge");
        project.tracks[0].set_annotation(10, "Intro");
        assert!(project.insert_bar_before(id, 0));
        let ticks: Vec<_> = project.tracks[0].annotations.iter().map(|note| note.tick).collect();
        assert_eq!(ticks, [width + 10, width * 2 + 10]);
        assert!(project.delete_bar(id, 0));
        assert!(project.delete_bar(id, 0));
        assert_eq!(project.tracks[0].annotations.len(), 1);
        assert_eq!(project.tracks[0].annotations[0].text, "Bridge");
        assert_eq!(project.tracks[0].annotations[0].tick, 10);
    }
}
