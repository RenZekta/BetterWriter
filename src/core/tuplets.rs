//! Tuplets. A span of a track is cut into equal parts that do not fit its
//! usual grid (three in the time of a quarter, five in a half, ...). The
//! span is stored on the track and lives only as long as it has notes:
//! notes of the span sit on its part boundaries and are whole parts long.
//! A note that does not (one moved onto the span, stretched across a part
//! boundary or out of the span, or as long as the whole span) overwrites it,
//! and the span goes. Parts that hold no note are rests.

// The editor and the file formats do not use most of this yet.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use super::project::{BwxProject, InstrumentTrack, TabNote, Tick};

/// Fewest and most parts a span can be cut into.
pub const MIN_PARTS: u8 = 2;
pub const MAX_PARTS: u8 = 64;
/// Shortest part, the length of the shortest note the editor writes.
pub const MIN_PART_TICKS: Tick = super::project::TICKS_PER_QUARTER / 32;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TupletSpan {
    pub start: Tick,
    pub length: Tick,
    pub parts: u8,
}

impl TupletSpan {
    pub fn end(&self) -> Tick {
        self.start + self.length
    }

    /// Where part `index` begins; `index == parts` is the span's end. Parts
    /// are as equal as whole ticks allow.
    pub fn boundary(&self, index: u32) -> Tick {
        let parts = Tick::from(self.parts.max(1));
        self.start + (self.length * Tick::from(index) + parts / 2) / parts
    }

    /// The part boundary `tick` is exactly on, as its index.
    pub fn boundary_index(&self, tick: Tick) -> Option<u32> {
        (0..=u32::from(self.parts)).find(|&index| self.boundary(index) == tick)
    }

    /// The part that holds `tick`.
    pub fn part_at(&self, tick: Tick) -> Option<u32> {
        (self.start..self.end())
            .contains(&tick)
            .then(|| (0..u32::from(self.parts)).rfind(|&index| self.boundary(index) <= tick))
            .flatten()
    }

    /// The part boundary nearest `tick`, clamped to the span.
    pub fn snap(&self, tick: Tick) -> Tick {
        (0..=u32::from(self.parts))
            .map(|index| self.boundary(index))
            .min_by_key(|boundary| (boundary - tick).abs())
            .unwrap_or(self.start)
    }

    /// How many notes of the usual kind the span's time holds: the largest
    /// power of two not above the number of parts (3 in the time of 2, 5 in
    /// 4, 7 in 4, 9 in 8), the "times" of a tuplet written `enters:times`.
    pub fn normal_notes(&self) -> u8 {
        1 << self.parts.max(1).ilog2()
    }

    /// Whether the parts really are a tuplet: a power-of-two split is only
    /// the usual note values.
    pub fn is_tuplet(&self) -> bool {
        !self.parts.is_power_of_two()
    }

    /// The length a note of `ticks` inside the span is written with: what
    /// the same note value would be outside any tuplet.
    pub fn written_ticks(&self, ticks: Tick) -> Tick {
        let enters = Tick::from(self.parts.max(1));
        let normal = Tick::from(self.normal_notes());
        (ticks * enters + normal / 2) / normal
    }

    /// The real length of `written` ticks of plain note values: the inverse
    /// of `written_ticks`.
    pub fn real_ticks(&self, written: Tick) -> Tick {
        let enters = Tick::from(self.parts.max(1));
        let normal = Tick::from(self.normal_notes());
        (written * normal + enters / 2) / enters
    }

    /// Whether `note` is one of the span's: inside it, on part boundaries and
    /// shorter than the whole span.
    pub fn holds(&self, note: &TabNote) -> bool {
        note.abs_tick >= self.start
            && note.end_tick() <= self.end()
            && note.duration_ticks < self.length
            && self.boundary_index(note.abs_tick).is_some()
            && self.boundary_index(note.end_tick()).is_some()
    }

    fn overlaps(&self, from: Tick, to: Tick) -> bool {
        from < self.end() && to > self.start
    }
}

impl InstrumentTrack {
    /// The span `tick` falls in, if any.
    pub fn tuplet_at(&self, tick: Tick) -> Option<&TupletSpan> {
        self.tuplets.iter().find(|span| (span.start..span.end()).contains(&tick))
    }

    /// The tuplet span `note` belongs to.
    pub fn tuplet_of(&self, note: &TabNote) -> Option<&TupletSpan> {
        self.tuplets.iter().find(|span| span.holds(note))
    }

    /// Drops the spans whose notes were overwritten and those with no note
    /// left (see the module comment). Returns whether any went.
    pub fn prune_tuplets(&mut self) -> bool {
        let before = self.tuplets.len();
        let notes = &self.notes;
        self.tuplets.retain(|span| {
            let mut holds_one = false;
            for note in notes.iter().filter(|note| span.overlaps(note.abs_tick, note.end_tick())) {
                if !span.holds(note) {
                    return false;
                }
                holds_one = true;
            }
            holds_one
        });
        self.tuplets.len() != before
    }

    /// Moves the spans at or after `from` by `delta` ticks, with the bars.
    pub fn shift_tuplets(&mut self, from: Tick, delta: Tick) {
        for span in &mut self.tuplets {
            if span.start >= from {
                span.start += delta;
            }
        }
    }

    /// Removes the spans that start in `from..to`.
    pub fn drop_tuplets_in(&mut self, from: Tick, to: Tick) {
        self.tuplets.retain(|span| span.start < from || span.start >= to);
    }

    /// Adds a span unless it would cross a barline, overlap another span, or
    /// its parts would be too short. An identical span is already there.
    pub fn add_tuplet(&mut self, span: TupletSpan) -> bool {
        if self.tuplets.contains(&span) {
            return true;
        }
        let (bar_start, bar_end) = self.measure_bounds_at(span.start);
        let valid = (MIN_PARTS..=MAX_PARTS).contains(&span.parts)
            && span.length / Tick::from(span.parts) >= MIN_PART_TICKS
            && span.start >= bar_start
            && span.end() <= bar_end
            && !self.tuplets.iter().any(|other| other.overlaps(span.start, span.end()));
        if valid {
            self.tuplets.push(span);
            self.tuplets.sort_by_key(|span| span.start);
        }
        valid
    }
}

impl BwxProject {
    /// Cuts each of the notes into `parts` equal notes of the same string and
    /// fret, and gives each its own span, as long as the note's stretch is
    /// free of other spans and inside one bar. Notes that start and end
    /// together (a chord) share one span. Returns the ids of the notes
    /// made, the first parts included.
    pub fn split_notes_into_tuplets(&mut self, track_id: u64, note_ids: &[u64], parts: u8) -> Vec<u64> {
        let mut made = Vec::new();
        for &id in note_ids {
            let Some(track) = self.track(track_id) else {
                return made;
            };
            let Some(note) = track.notes.iter().find(|note| note.id == id).cloned() else {
                continue;
            };
            let span = TupletSpan {
                start: note.abs_tick,
                length: note.duration_ticks,
                parts,
            };
            if track.tuplet_of(&note).is_some() {
                continue;
            }
            let extra: Vec<u64> = (1..parts).map(|_| self.allocate_note_id()).collect();
            let Some(track) = self.track_mut(track_id) else {
                return made;
            };
            if !track.add_tuplet(span) {
                continue;
            }
            let Some(at) = track.notes.iter().position(|candidate| candidate.id == id) else {
                continue;
            };
            let mut first = track.notes[at].clone();
            first.duration_ticks = span.boundary(1) - span.boundary(0);
            let mut parts_made = vec![first.clone()];
            for index in 1..u32::from(parts) {
                let mut next = first.clone();
                next.id = extra[index as usize - 1];
                next.abs_tick = span.boundary(index);
                next.duration_ticks = span.boundary(index + 1) - span.boundary(index);
                next.tied_from_previous = None;
                parts_made.push(next);
            }
            made.extend(parts_made.iter().map(|part| part.id));
            track.notes.splice(at..=at, parts_made);
            track.sort_notes();
        }
        made
    }

    /// Writes one note into every part of a new span: the cursor split into
    /// `parts` with a note typed into it. Fails when something overlaps the
    /// stretch on that string or the span does not fit.
    #[allow(clippy::too_many_arguments)]
    pub fn write_tuplet(
        &mut self,
        track_id: u64,
        start: Tick,
        length: Tick,
        parts: u8,
        string_index: usize,
        fret: u8,
        velocity: u8,
    ) -> Vec<u64> {
        let span = TupletSpan { start, length, parts };
        let ids: Vec<u64> = (0..parts).map(|_| self.allocate_note_id()).collect();
        let Some(track) = self.track_mut(track_id) else {
            return Vec::new();
        };
        let string_index = string_index.min(track.tuning.len().saturating_sub(1));
        let blocked = track.notes.iter().any(|note| {
            note.string_index == string_index && note.abs_tick < span.end() && note.end_tick() > span.start
        });
        if blocked || !track.add_tuplet(span) {
            return Vec::new();
        }
        let fret = fret.min(track.fret_count);
        for (index, id) in ids.iter().enumerate() {
            let index = index as u32;
            track.notes.push(TabNote {
                id: *id,
                abs_tick: span.boundary(index),
                duration_ticks: span.boundary(index + 1) - span.boundary(index),
                string_index,
                fret,
                velocity,
                effects: Default::default(),
                tied_from_previous: None,
            });
        }
        track.sort_notes();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::project::TICKS_PER_QUARTER as Q;

    fn project_with_notes(notes: &[(Tick, Tick, usize)]) -> (BwxProject, u64, Vec<u64>) {
        let mut project = BwxProject::demo();
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        project.tracks[0].tuplets.clear();
        let ids = notes
            .iter()
            .map(|&(start, length, string)| project.insert_note(id, start, length, string, 3).unwrap())
            .collect();
        (project, id, ids)
    }

    #[test]
    fn boundaries_split_the_span_as_evenly_as_ticks_allow() {
        let triplet = TupletSpan { start: 960, length: 960, parts: 3 };
        assert_eq!((0..=3).map(|i| triplet.boundary(i)).collect::<Vec<_>>(), [960, 1280, 1600, 1920]);
        let septuplet = TupletSpan { start: 0, length: 960, parts: 7 };
        let lengths: Vec<Tick> = (0..7).map(|i| septuplet.boundary(i + 1) - septuplet.boundary(i)).collect();
        assert_eq!(lengths.iter().sum::<Tick>(), 960);
        assert!(lengths.iter().all(|&l| (137..=138).contains(&l)));
        assert_eq!(triplet.boundary_index(1600), Some(2));
        assert_eq!(triplet.boundary_index(1500), None);
        assert_eq!(triplet.part_at(1500), Some(1));
        assert_eq!(triplet.snap(1500), 1600);
    }

    #[test]
    fn the_normal_side_is_the_power_of_two_below_the_parts() {
        let normal = |parts| TupletSpan { start: 0, length: 960, parts }.normal_notes();
        assert_eq!([2, 3, 4, 5, 6, 7, 8, 9, 12].map(normal), [2, 2, 4, 4, 4, 4, 8, 8, 8]);
        let triplet = TupletSpan { start: 0, length: 960, parts: 3 };
        assert_eq!(triplet.written_ticks(320), 480);
        assert_eq!(triplet.written_ticks(640), 960);
        assert!(triplet.is_tuplet() && !TupletSpan { start: 0, length: 960, parts: 4 }.is_tuplet());
    }

    #[test]
    fn a_note_splits_into_equal_parts_with_a_span_of_its_own() {
        let (mut project, id, ids) = project_with_notes(&[(0, Q, 0), (Q, Q, 0)]);
        let made = project.split_notes_into_tuplets(id, &ids[..1], 3);
        assert_eq!(made.len(), 3);
        let track = &project.tracks[0];
        let spans: Vec<TupletSpan> = track.tuplets.clone();
        assert_eq!(spans, [TupletSpan { start: 0, length: Q, parts: 3 }]);
        let notes: Vec<(Tick, Tick)> = track.notes.iter().map(|n| (n.abs_tick, n.duration_ticks)).collect();
        assert_eq!(notes, [(0, 320), (320, 320), (640, 320), (Q, Q)]);
        assert!(track.notes.iter().take(3).all(|n| track.tuplet_of(n).is_some()));
        assert!(track.tuplet_of(&track.notes[3]).is_none());
    }

    #[test]
    fn a_chord_shares_one_span_and_a_neighbouring_span_may_not_overlap() {
        let (mut project, id, ids) = project_with_notes(&[(0, Q, 0), (0, Q, 1), (Q / 2, Q, 2)]);
        let made = project.split_notes_into_tuplets(id, &ids[..2], 3);
        assert_eq!(made.len(), 6);
        assert_eq!(project.tracks[0].tuplets.len(), 1);
        let overlapping = project.split_notes_into_tuplets(id, &ids[2..], 3);
        assert!(overlapping.is_empty());
    }

    #[test]
    fn a_span_may_not_cross_a_barline() {
        let (mut project, id, ids) = project_with_notes(&[(Q * 3, Q * 2, 0)]);
        assert!(project.split_notes_into_tuplets(id, &ids, 3).is_empty());
        assert!(project.tracks[0].tuplets.is_empty());
    }

    #[test]
    fn the_span_goes_when_a_normal_note_lands_on_it_or_its_notes_are_gone() {
        let (mut project, id, ids) = project_with_notes(&[(0, Q, 0)]);
        project.split_notes_into_tuplets(id, &ids, 3);
        let track = &mut project.tracks[0];
        // A rest inside the span keeps it.
        track.notes.remove(1);
        assert!(!track.prune_tuplets());
        // A note moved off the part boundaries overwrites it.
        track.notes[1].abs_tick += 40;
        assert!(track.prune_tuplets());
        assert!(track.tuplets.is_empty());

        let (mut project, id, ids) = project_with_notes(&[(0, Q, 0)]);
        project.split_notes_into_tuplets(id, &ids, 4);
        let track = &mut project.tracks[0];
        // Stretched over the whole span: a plain quarter again.
        track.notes.truncate(1);
        track.notes[0].duration_ticks = Q;
        assert!(track.prune_tuplets());

        let (mut project, id, ids) = project_with_notes(&[(0, Q, 0)]);
        project.split_notes_into_tuplets(id, &ids, 3);
        let track = &mut project.tracks[0];
        track.notes.clear();
        assert!(track.prune_tuplets());
    }

    #[test]
    fn two_parts_together_are_still_a_tuplet_note() {
        let (mut project, id, ids) = project_with_notes(&[(0, Q, 0)]);
        project.split_notes_into_tuplets(id, &ids, 3);
        let track = &mut project.tracks[0];
        track.notes.truncate(2);
        track.notes[0].duration_ticks = 640;
        track.notes.remove(1);
        assert!(!track.prune_tuplets());
        assert!(track.tuplet_of(&track.notes[0]).is_some());
    }

    #[test]
    fn triplet_notes_are_written_as_eighths_in_one_piece() {
        let (mut project, id, ids) = project_with_notes(&[(0, Q, 0)]);
        project.split_notes_into_tuplets(id, &ids, 3);
        let track = &project.tracks[0];
        assert_eq!(crate::core::ties::note_slices(track, 320, 640), [(320, 640)]);
        // Two parts together are a quarter of the triplet.
        assert_eq!(crate::core::ties::note_slices(track, 0, 640), [(0, 640)]);
        let (start, end) = track.measure_bounds_at(0);
        let rests = track.bar_rests(0, crate::core::edit::RestFill::AllGaps { preferred: Q });
        let clef = crate::notation::Clef::for_track(track);
        let score = crate::notation::score_bar(track, clef, start, end, &rests, Q);
        let values: Vec<_> = score
            .items
            .iter()
            .filter_map(|item| match item {
                crate::notation::ScoreItem::Chord(chord) => Some(chord.value.value),
                crate::notation::ScoreItem::Rest { .. } => None,
            })
            .collect();
        assert_eq!(values, [8, 8, 8]);
        assert_eq!(score.beams.len(), 1);
        assert_eq!(score.beams[0].items.len(), 3);
    }

    #[test]
    fn empty_parts_rest_part_by_part() {
        let (mut project, id, ids) = project_with_notes(&[(0, Q, 0)]);
        project.split_notes_into_tuplets(id, &ids, 3);
        let track = &mut project.tracks[0];
        track.notes.remove(1);
        let rests = track.bar_rests(0, crate::core::edit::RestFill::AllGaps { preferred: Q });
        assert!(rests.iter().any(|rest| rest.abs_tick == 320 && rest.duration_ticks == 320));
        assert!(!rests.iter().any(|rest| rest.abs_tick < Q && rest.duration_ticks == Q));
        let sum: Tick = rests.iter().map(|rest| rest.duration_ticks).sum();
        assert_eq!(sum + 640, track.measure_bounds_at(0).1);
    }

    #[test]
    fn a_tuplet_note_stretches_by_whole_parts() {
        let (mut project, id, ids) = project_with_notes(&[(0, Q, 0)]);
        let made = project.split_notes_into_tuplets(id, &ids, 3);
        // Drag the first part's right edge across the (emptied) second part.
        project.delete_note(id, made[1]);
        assert!(project.stretch_note(id, made[0], crate::core::edit::StretchEdge::Right, 250, 60));
        assert_eq!(project.tracks[0].notes[0].duration_ticks, 640);
        assert_eq!(project.tracks[0].tuplets.len(), 1);
        // Further would reach the next part's note: nothing to take.
        assert!(!project.stretch_note(id, made[0], crate::core::edit::StretchEdge::Right, 100, 60));
    }

    #[test]
    fn writing_into_a_split_cursor_fills_every_part() {
        let (mut project, id, _) = project_with_notes(&[]);
        let ids = project.write_tuplet(id, Q, Q, 5, 1, 7, 100);
        assert_eq!(ids.len(), 5);
        let track = &project.tracks[0];
        assert_eq!(track.notes.len(), 5);
        assert_eq!(track.notes.iter().map(|n| n.duration_ticks).sum::<Tick>(), Q);
        assert!(project.write_tuplet(id, Q, Q, 3, 1, 7, 100).is_empty());
    }

    #[test]
    fn bar_edits_carry_the_spans() {
        let (mut project, id, ids) = project_with_notes(&[(Q * 4, Q, 0)]);
        project.split_notes_into_tuplets(id, &ids, 3);
        let width = project.tracks[0].measure_bounds_at(0).1;
        assert!(project.insert_bar_before(id, 0));
        assert_eq!(project.tracks[0].tuplets[0].start, Q * 4 + width);
        assert!(project.delete_bar(id, 0));
        assert_eq!(project.tracks[0].tuplets[0].start, Q * 4);
        assert!(project.delete_bar(id, Q * 4));
        assert!(project.tracks[0].tuplets.is_empty());
    }
}
