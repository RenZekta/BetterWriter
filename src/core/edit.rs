use super::project::{
    BwxProject, NoteEffects, RestNode, TICKS_PER_QUARTER, TabNote, TempoPoint, TempoTransition,
    Tick, TimeSignature,
};

/// Which edge of a note block a stretch drag is grabbing (see
/// `BwxProject::stretch_note`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StretchEdge {
    Left,
    Right,
}

impl BwxProject {
    /// Inserts a note, shrinking its duration as needed to avoid overlapping
    /// another note already sounding on the *same string* — an overlapping
    /// *earlier* note on that string gets cut short right here, and this new
    /// note itself gets cut short if a *later* one on that string already
    /// starts before it would end. Notes on different strings (chords) are
    /// unaffected.
    ///
    /// A note's duration is *not* clamped to the bar it starts in: a note
    /// can legitimately span past a bar boundary (this doubles as this
    /// app's stand-in for a tied note, until real tie support exists — see
    /// Plan.md). Rendering splits such a note visually at the boundary
    /// instead of letting it float or overlap incorrectly; see
    /// `BetterWriterApp::paint_note_fragment`.
    pub fn insert_note(
        &mut self,
        track_id: u64,
        abs_tick: Tick,
        duration_ticks: Tick,
        string_index: usize,
        fret: u8,
    ) -> Option<u64> {
        let id = self.allocate_note_id();
        let track = self.track_mut(track_id)?;
        let string_index = string_index.min(track.tuning.len().saturating_sub(1));
        let abs_tick = abs_tick.max(0);
        let minimum_duration = TICKS_PER_QUARTER / 32;
        let mut duration_ticks = duration_ticks.max(minimum_duration);

        if let Some(next_start) = track
            .notes
            .iter()
            .filter(|note| note.string_index == string_index && note.abs_tick > abs_tick)
            .map(|note| note.abs_tick)
            .min()
        {
            duration_ticks = duration_ticks.min((next_start - abs_tick).max(1));
        }
        for note in track.notes.iter_mut() {
            if note.string_index == string_index
                && note.abs_tick < abs_tick
                && note.abs_tick + note.duration_ticks > abs_tick
            {
                note.duration_ticks = (abs_tick - note.abs_tick).max(1);
            }
        }

        let bounded_fret = fret.min(track.fret_count);
        track.notes.push(TabNote {
            id,
            abs_tick,
            duration_ticks,
            string_index,
            fret: bounded_fret,
            velocity: 108,
            effects: NoteEffects::default(),
        });
        track.sort_notes();
        Some(id)
    }

    pub fn delete_note(&mut self, track_id: u64, note_id: u64) -> bool {
        let Some(track) = self.track_mut(track_id) else {
            return false;
        };
        let before = track.notes.len();
        track.notes.retain(|note| note.id != note_id);
        before != track.notes.len()
    }

    /// Insert or replace a time signature change on one track. Each change
    /// begins a fresh bar grid at `at_tick`, allowing genuinely independent
    /// (polymetric) track timelines.
    pub fn set_track_time_signature(
        &mut self,
        track_id: u64,
        at_tick: Tick,
        signature: TimeSignature,
    ) -> bool {
        if signature.numerator == 0 || signature.denominator == 0 {
            return false;
        }
        let Some(track) = self.track_mut(track_id) else {
            return false;
        };
        let at_tick = at_tick.max(0);
        if let Some(change) = track
            .time_signature_changes
            .iter_mut()
            .find(|change| change.at_tick == at_tick)
        {
            if change.signature == signature {
                return false;
            }
            change.signature = signature;
        } else {
            track
                .time_signature_changes
                .push(super::project::TimeSignatureChange { at_tick, signature });
            track
                .time_signature_changes
                .sort_by_key(|change| change.at_tick);
        }
        true
    }

    /// Insert or replace a point on the project-wide tempo automation
    /// timeline. Tempo is global (not per-track), unlike time signatures.
    pub fn set_tempo_point(
        &mut self,
        at_tick: Tick,
        bpm: f32,
        transition: TempoTransition,
    ) -> bool {
        if !bpm.is_finite() || bpm <= 0.0 {
            return false;
        }
        let at_tick = at_tick.max(0);
        let bpm = bpm.clamp(1.0, 999.0);
        if let Some(point) = self
            .tempo_points
            .iter_mut()
            .find(|point| point.at_tick == at_tick)
        {
            if point.bpm == bpm && point.transition == transition {
                return false;
            }
            point.bpm = bpm;
            point.transition = transition;
        } else {
            self.tempo_points.push(TempoPoint {
                at_tick,
                bpm,
                transition,
                hidden: false,
            });
            self.tempo_points.sort_by_key(|point| point.at_tick);
        }
        true
    }

    /// Toggle whether a tempo point's "♩=bpm" label is drawn above the staff.
    pub fn set_tempo_point_hidden(&mut self, at_tick: Tick, hidden: bool) -> bool {
        let Some(point) = self
            .tempo_points
            .iter_mut()
            .find(|point| point.at_tick == at_tick)
        else {
            return false;
        };
        if point.hidden == hidden {
            return false;
        }
        point.hidden = hidden;
        true
    }

    /// Removes the tempo point at `at_tick`. Refuses to remove the point at
    /// (or before) tick 0, since playback always needs a starting tempo.
    pub fn delete_tempo_point(&mut self, at_tick: Tick) -> bool {
        if at_tick <= 0 {
            return false;
        }
        let before = self.tempo_points.len();
        self.tempo_points.retain(|point| point.at_tick != at_tick);
        self.tempo_points.len() != before
    }

    /// Resets tempo automation back to a single flat point, mirroring Guitar
    /// Pro's "Remove automations".
    pub fn clear_tempo_automation(&mut self, bpm: f32) {
        self.tempo_points = vec![TempoPoint::new(0, bpm)];
    }

    pub fn set_note_duration_fluid(
        &mut self,
        track_id: u64,
        note_id: u64,
        new_duration_ticks: Tick,
    ) -> bool {
        let Some(track) = self.track_mut(track_id) else {
            return false;
        };
        let Some(index) = track.notes.iter().position(|note| note.id == note_id) else {
            return false;
        };

        let (measure_start, measure_end) = track.measure_bounds_at(track.notes[index].abs_tick);
        let old_duration = track.notes[index].duration_ticks;
        let delta = new_duration_ticks - old_duration;
        let new_end = track.notes[index].abs_tick + new_duration_ticks;
        if new_end > measure_end || new_duration_ticks <= 0 {
            return false;
        }

        track.notes[index].duration_ticks = new_duration_ticks;
        if delta != 0 {
            let pivot = track.notes[index].abs_tick;
            for note in &mut track.notes {
                if note.id != note_id
                    && note.abs_tick > pivot
                    && note.abs_tick >= measure_start
                    && note.abs_tick < measure_end
                {
                    note.abs_tick = (note.abs_tick + delta).min(measure_end).max(measure_start);
                }
            }
        }

        track.sort_notes();
        true
    }

    /// Resizes a note by dragging its left or right edge, in whole
    /// `step_ticks` increments. Dragging the **right** edge changes only
    /// the duration (the start stays put); dragging the **left** edge moves
    /// the start while keeping the *end* tick fixed, so the note extends or
    /// retreats from that side instead.
    ///
    /// Like `insert_note`, the result is clamped so it can't overlap another
    /// note on the same string — but (also like `insert_note`, and unlike
    /// `set_note_duration_fluid`) it's free to cross a bar boundary; that's
    /// this app's stand-in for a tied note until real ties exist.
    pub fn stretch_note(
        &mut self,
        track_id: u64,
        note_id: u64,
        edge: StretchEdge,
        delta_ticks: Tick,
        step_ticks: Tick,
    ) -> bool {
        let step_ticks = step_ticks.max(1);
        // Round toward zero to the nearest whole step, so a drag that
        // hasn't crossed a full step yet does nothing rather than jittering.
        let delta_ticks = (delta_ticks / step_ticks) * step_ticks;
        if delta_ticks == 0 {
            return false;
        }

        let Some(track) = self.track_mut(track_id) else {
            return false;
        };
        let Some(index) = track.notes.iter().position(|note| note.id == note_id) else {
            return false;
        };
        let original = track.notes[index].clone();
        let minimum_duration = TICKS_PER_QUARTER / 32;

        match edge {
            StretchEdge::Right => {
                let mut new_duration =
                    (original.duration_ticks + delta_ticks).max(minimum_duration);
                if let Some(next_start) = track
                    .notes
                    .iter()
                    .filter(|note| {
                        note.id != note_id
                            && note.string_index == original.string_index
                            && note.abs_tick > original.abs_tick
                    })
                    .map(|note| note.abs_tick)
                    .min()
                {
                    new_duration =
                        new_duration.min((next_start - original.abs_tick).max(minimum_duration));
                }
                if new_duration == original.duration_ticks {
                    return false;
                }
                track.notes[index].duration_ticks = new_duration;
            }
            StretchEdge::Left => {
                let note_end = original.abs_tick + original.duration_ticks;
                let mut new_start = (original.abs_tick + delta_ticks).max(0);
                if let Some(prev_end) = track
                    .notes
                    .iter()
                    .filter(|note| {
                        note.id != note_id
                            && note.string_index == original.string_index
                            && note.abs_tick < original.abs_tick
                    })
                    .map(|note| note.abs_tick + note.duration_ticks)
                    .max()
                {
                    new_start = new_start.max(prev_end);
                }
                new_start = new_start.min(note_end - minimum_duration);
                if new_start == original.abs_tick {
                    return false;
                }
                track.notes[index].abs_tick = new_start;
                track.notes[index].duration_ticks = note_end - new_start;
            }
        }

        track.sort_notes();
        true
    }

    /// Moves a note earlier/later in time and/or to a different string,
    /// keeping its duration fixed. Clamped so it can't overlap another note
    /// on the target string — except any note listed in `excluded_ids` (the
    /// rest of a multi-note drag moving together by the same amount, so
    /// they don't clamp against each other mid-gesture) — and so the string
    /// stays in range. Free to cross a bar boundary, same as
    /// `insert_note`/`stretch_note`.
    pub fn move_note(
        &mut self,
        track_id: u64,
        note_id: u64,
        delta_ticks: Tick,
        string_delta: i32,
        excluded_ids: &[u64],
    ) -> bool {
        let Some(track) = self.track_mut(track_id) else {
            return false;
        };
        let Some(index) = track.notes.iter().position(|note| note.id == note_id) else {
            return false;
        };
        let original = track.notes[index].clone();
        let max_string_index = track.tuning.len().saturating_sub(1) as i32;
        let new_string_index =
            (original.string_index as i32 + string_delta).clamp(0, max_string_index) as usize;

        if delta_ticks == 0 && new_string_index == original.string_index {
            return false;
        }
        let tentative_abs_tick = (original.abs_tick + delta_ticks).max(0);
        let tentative_end = tentative_abs_tick + original.duration_ticks;

        // If the tentative position overlaps a note already on the target
        // string, resolve it by pushing to whichever side (right before
        // that note, or right after it) is closer to the tentative
        // position — rather than separately bounding by "the nearest note
        // before" and "the nearest note after", which can agree on the same
        // point and leave the note sitting right on top of the obstacle
        // when the tentative position lands inside it.
        let obstacle = track.notes.iter().find(|note| {
            note.id != note_id
                && !excluded_ids.contains(&note.id)
                && note.string_index == new_string_index
                && note.abs_tick < tentative_end
                && note.abs_tick + note.duration_ticks > tentative_abs_tick
        });
        let new_abs_tick = match obstacle {
            Some(obstacle) => {
                // "Push before" only makes sense if there's actually room
                // there — an obstacle sitting at (or near) tick 0 leaves
                // none, in which case the only valid resolution is after it.
                let push_before_valid = obstacle.abs_tick >= original.duration_ticks;
                let push_after = obstacle.abs_tick + obstacle.duration_ticks;
                if push_before_valid {
                    let push_before = obstacle.abs_tick - original.duration_ticks;
                    let distance_before = (tentative_abs_tick - push_before).abs();
                    let distance_after = (push_after - tentative_abs_tick).abs();
                    if distance_before <= distance_after {
                        push_before
                    } else {
                        push_after
                    }
                } else {
                    push_after
                }
            }
            None => tentative_abs_tick,
        };

        if new_abs_tick == original.abs_tick && new_string_index == original.string_index {
            return false;
        }
        track.notes[index].abs_tick = new_abs_tick;
        track.notes[index].string_index = new_string_index;
        track.sort_notes();
        true
    }

    /// Insert a new bar to the left of the bar containing `at_tick`, on the
    /// given track only (bars are per-track, so polymetric projects insert
    /// independently per instrument). The new bar duplicates the selected
    /// bar's current time signature; the selected bar and everything after
    /// it simply shift right by the new bar's width, unaffected otherwise.
    pub fn insert_bar_before(&mut self, track_id: u64, at_tick: Tick) -> bool {
        let Some(track) = self.track_mut(track_id) else {
            return false;
        };
        let (bar_start, bar_end) = track.measure_bounds_at(at_tick.max(0));
        let width = bar_end - bar_start;
        if width <= 0 {
            return false;
        }
        // Notes at or after the bar start move into the new bar's slot.
        // Signature changes strictly after the bar start move too, but a
        // change sitting exactly at `bar_start` defines the bar being
        // duplicated, so it stays put and now also governs the new bar.
        for note in &mut track.notes {
            if note.abs_tick >= bar_start {
                note.abs_tick += width;
            }
        }
        for change in &mut track.time_signature_changes {
            if change.at_tick > bar_start {
                change.at_tick += width;
            }
        }
        track.bar_count = track.bar_count.saturating_add(1);
        track.sort_notes();
        true
    }

    /// Add a new, empty bar to the right of the bar containing `at_tick`,
    /// inheriting that bar's time signature. Everything from the next bar
    /// onward shifts right by the new bar's width.
    pub fn add_bar_after(&mut self, track_id: u64, at_tick: Tick) -> bool {
        let Some(track) = self.track_mut(track_id) else {
            return false;
        };
        let (bar_start, bar_end) = track.measure_bounds_at(at_tick.max(0));
        let width = bar_end - bar_start;
        if width <= 0 {
            return false;
        }
        // Everything at or after the *next* bar's start shifts right,
        // including a signature change sitting exactly there (which still
        // governs the old next bar, just later); the newly opened gap has no
        // change of its own so it inherits the selected bar's signature.
        for note in &mut track.notes {
            if note.abs_tick >= bar_end {
                note.abs_tick += width;
            }
        }
        for change in &mut track.time_signature_changes {
            if change.at_tick >= bar_end {
                change.at_tick += width;
            }
        }
        track.bar_count = track.bar_count.saturating_add(1);
        track.sort_notes();
        true
    }

    /// Delete the bar containing `at_tick`, along with any notes inside it.
    /// Refuses to drop the last remaining bar. Returns `false` on failure.
    pub fn delete_bar(&mut self, track_id: u64, at_tick: Tick) -> bool {
        let Some(track) = self.track_mut(track_id) else {
            return false;
        };
        if track.bar_count <= 1 {
            return false;
        }
        let (bar_start, bar_end) = track.measure_bounds_at(at_tick.max(0));
        let width = bar_end - bar_start;
        if width <= 0 {
            return false;
        }
        // The signature that will now take over at `bar_start` once the bar
        // is gone, so tick 0 never ends up without a governing signature.
        let inheriting_signature = track.signature_at(bar_end);

        track
            .notes
            .retain(|note| note.abs_tick < bar_start || note.abs_tick >= bar_end);
        track
            .time_signature_changes
            .retain(|change| change.at_tick < bar_start || change.at_tick >= bar_end);
        for note in &mut track.notes {
            if note.abs_tick >= bar_end {
                note.abs_tick -= width;
            }
        }
        for change in &mut track.time_signature_changes {
            if change.at_tick >= bar_end {
                change.at_tick -= width;
            }
        }
        if track
            .time_signature_changes
            .first()
            .map(|change| change.at_tick)
            != Some(0)
        {
            track.time_signature_changes.insert(
                0,
                super::project::TimeSignatureChange {
                    at_tick: 0,
                    signature: inheriting_signature,
                },
            );
        }
        track.time_signature_changes.sort_by_key(|c| c.at_tick);

        track.bar_count -= 1;
        track.sort_notes();
        true
    }

    pub fn tail_padding_rests(&self, track_id: u64, measure_tick: Tick) -> Vec<RestNode> {
        let Some(track) = self.track(track_id) else {
            return Vec::new();
        };
        let (_, measure_end) = track.measure_bounds_at(measure_tick);
        let last_end = track
            .notes
            .iter()
            .filter(|note| {
                let (start, end) = track.measure_bounds_at(note.abs_tick);
                measure_tick >= start && measure_tick < end
            })
            .map(TabNote::end_tick)
            .max()
            .unwrap_or_else(|| track.measure_bounds_at(measure_tick).0);

        split_rest_gap(last_end.max(measure_tick), measure_end)
    }
}

pub fn split_rest_gap(start_tick: Tick, end_tick: Tick) -> Vec<RestNode> {
    let mut rests = Vec::new();
    let mut cursor = start_tick;
    let standard = [
        TICKS_PER_QUARTER * 4,
        TICKS_PER_QUARTER * 2,
        TICKS_PER_QUARTER,
        TICKS_PER_QUARTER / 2,
        TICKS_PER_QUARTER / 4,
        TICKS_PER_QUARTER / 8,
        TICKS_PER_QUARTER / 16,
        TICKS_PER_QUARTER / 32,
    ];

    while cursor < end_tick {
        let remaining = end_tick - cursor;
        let duration = standard
            .iter()
            .copied()
            .find(|ticks| *ticks <= remaining)
            .unwrap_or(remaining);
        rests.push(RestNode {
            abs_tick: cursor,
            duration_ticks: duration,
        });
        cursor += duration;
    }

    rests
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{DurationChoice, TICKS_PER_QUARTER};

    #[test]
    fn stretch_note_right_edge_grows_and_shrinks_duration() {
        let mut project = BwxProject::demo();
        let step = TICKS_PER_QUARTER / 4;
        // Note id 1: abs_tick 0, duration TICKS_PER_QUARTER, string 5.
        assert!(project.stretch_note(1, 1, StretchEdge::Right, step, step));
        let track = project.track(1).unwrap();
        let note = track.notes.iter().find(|n| n.id == 1).unwrap();
        assert_eq!(note.abs_tick, 0);
        assert_eq!(note.duration_ticks, TICKS_PER_QUARTER + step);

        assert!(project.stretch_note(1, 1, StretchEdge::Right, -step * 2, step));
        let track = project.track(1).unwrap();
        let note = track.notes.iter().find(|n| n.id == 1).unwrap();
        assert_eq!(note.duration_ticks, TICKS_PER_QUARTER - step);
    }

    #[test]
    fn stretch_note_left_edge_moves_start_and_keeps_end_fixed() {
        let mut project = BwxProject::demo();
        let step = TICKS_PER_QUARTER / 4;
        // Note id 2: abs_tick TICKS_PER_QUARTER, duration TICKS_PER_QUARTER/2, string 4.
        let original_end = TICKS_PER_QUARTER + TICKS_PER_QUARTER / 2;
        assert!(project.stretch_note(1, 2, StretchEdge::Left, -step, step));
        let track = project.track(1).unwrap();
        let note = track.notes.iter().find(|n| n.id == 2).unwrap();
        assert_eq!(note.abs_tick, TICKS_PER_QUARTER - step);
        assert_eq!(note.abs_tick + note.duration_ticks, original_end);
    }

    #[test]
    fn stretch_note_right_edge_refuses_to_cross_a_later_note_on_the_same_string() {
        let mut project = BwxProject::demo();
        let step = TICKS_PER_QUARTER / 4;
        project.insert_note(1, 0, step, 1, 3).unwrap();
        let later_id = project
            .insert_note(1, step * 2, step, 1, 5)
            .unwrap();
        let first_id = project
            .track(1)
            .unwrap()
            .notes
            .iter()
            .find(|n| n.string_index == 1 && n.abs_tick == 0)
            .unwrap()
            .id;
        // Growing far enough to reach the later note should clamp right at
        // its start, not cross over it.
        assert!(project.stretch_note(1, first_id, StretchEdge::Right, step * 10, step));
        let track = project.track(1).unwrap();
        let grown = track.notes.iter().find(|n| n.id == first_id).unwrap();
        assert_eq!(grown.abs_tick + grown.duration_ticks, step * 2);
        let later = track.notes.iter().find(|n| n.id == later_id).unwrap();
        assert_eq!(later.abs_tick, step * 2);
    }

    #[test]
    fn stretch_note_right_edge_can_cross_a_bar_boundary() {
        let mut project = BwxProject::demo();
        let step = TICKS_PER_QUARTER / 4;
        let (_, bar_end) = project.track(1).unwrap().measure_bounds_at(0);
        let original_duration = project
            .track(1)
            .unwrap()
            .notes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .duration_ticks;
        // Note id 1 starts at tick 0; growing it far past the bar end is
        // allowed now (this app's stand-in for a tied note — see
        // `insert_note_can_span_past_a_bar_boundary` below).
        assert!(project.stretch_note(1, 1, StretchEdge::Right, bar_end * 2, step));
        let track = project.track(1).unwrap();
        let note = track.notes.iter().find(|n| n.id == 1).unwrap();
        assert_eq!(note.duration_ticks, original_duration + bar_end * 2);
        assert!(note.abs_tick + note.duration_ticks > bar_end);
    }

    #[test]
    fn move_note_shifts_it_and_can_cross_a_bar_boundary() {
        let mut project = BwxProject::demo();
        let (_, bar_end) = project.track(1).unwrap().measure_bounds_at(0);
        assert!(project.move_note(1, 1, bar_end + 100, 0, &[]));
        let track = project.track(1).unwrap();
        let note = track.notes.iter().find(|n| n.id == 1).unwrap();
        assert_eq!(note.abs_tick, bar_end + 100);
    }

    #[test]
    fn move_note_clamps_against_a_later_note_on_the_same_string() {
        let mut project = BwxProject::demo();
        let step = TICKS_PER_QUARTER / 4;
        let moving_id = project.insert_note(1, 0, step, 1, 3).unwrap();
        let anchor_id = project.insert_note(1, step * 4, step, 1, 5).unwrap();
        // Move exactly far enough to land squarely on the anchor note.
        assert!(project.move_note(1, moving_id, step * 4, 0, &[]));
        let track = project.track(1).unwrap();
        let moved = track.notes.iter().find(|n| n.id == moving_id).unwrap();
        let anchor = track.notes.iter().find(|n| n.id == anchor_id).unwrap();
        // Landing exactly on the anchor's start is equidistant either way;
        // ties resolve to the earlier side.
        assert_eq!(moved.abs_tick + moved.duration_ticks, anchor.abs_tick);
    }

    #[test]
    fn move_note_ignores_excluded_ids_so_a_group_can_move_together() {
        let mut project = BwxProject::demo();
        let step = TICKS_PER_QUARTER / 4;
        let a = project.insert_note(1, 0, step, 1, 3).unwrap();
        let b = project.insert_note(1, step * 2, step, 1, 5).unwrap();
        // Moving `a` right past where `b` currently sits would normally
        // clamp against it — excluding `b` (as if it's moving together with
        // `a` in the same drag) lets it pass through instead.
        assert!(project.move_note(1, a, step * 3, 0, &[b]));
        let track = project.track(1).unwrap();
        let moved = track.notes.iter().find(|n| n.id == a).unwrap();
        assert_eq!(moved.abs_tick, step * 3);
    }

    #[test]
    fn move_note_can_change_string_and_clamps_string_to_range() {
        let mut project = BwxProject::demo();
        let track_id = project.tracks[0].id;

        // Note id 1 is on string 5 (the last/lowest string); moving it down
        // by 2 strings lands on string 3.
        assert!(project.move_note(track_id, 1, 0, -2, &[]));
        let track = project.track(track_id).unwrap();
        let note = track.notes.iter().find(|n| n.id == 1).unwrap();
        assert_eq!(note.string_index, 3);

        // Pushing far past the top clamps to string 0 instead of panicking
        // or wrapping.
        assert!(project.move_note(track_id, 1, 0, -99, &[]));
        let track = project.track(track_id).unwrap();
        let note = track.notes.iter().find(|n| n.id == 1).unwrap();
        assert_eq!(note.string_index, 0);
    }

    #[test]
    fn move_note_avoids_overlap_on_the_string_it_moves_onto() {
        let mut project = BwxProject::demo();
        let step = TICKS_PER_QUARTER / 4;
        // An obstacle sitting on string 2, right where the moved note would
        // otherwise land.
        let obstacle_id = project.insert_note(1, 0, step * 2, 2, 7).unwrap();
        let moving_id = project.insert_note(1, 0, step, 3, 3).unwrap();
        // Move from string 3 onto string 2 (delta -1) — should shrink/clamp
        // against the obstacle rather than overlapping it.
        assert!(project.move_note(1, moving_id, 0, -1, &[]));
        let track = project.track(1).unwrap();
        let moved = track.notes.iter().find(|n| n.id == moving_id).unwrap();
        let obstacle = track.notes.iter().find(|n| n.id == obstacle_id).unwrap();
        assert_eq!(moved.string_index, 2);
        assert!(moved.abs_tick >= obstacle.abs_tick + obstacle.duration_ticks);
    }

    #[test]
    fn insert_note_can_span_past_a_bar_boundary() {
        let mut project = BwxProject::empty_with_track(
            "Solo",
            crate::core::InstrumentFamily::Stringed,
            crate::core::StringedVariant::AcousticGuitar,
            6,
        );
        let track_id = project.tracks[0].id;
        // A 4/4 bar is 3840 ticks. A whole note (3840 ticks) starting at
        // tick 2000 ends at 5840, well past the bar — and that's allowed:
        // this is this app's stand-in for a tied note until real ties
        // exist. Rendering is responsible for splitting it visually at the
        // boundary (see `paint_note_fragment`), not the data model.
        let id = project
            .insert_note(track_id, 2_000, TICKS_PER_QUARTER * 4, 0, 3)
            .unwrap();
        let note = project
            .track(track_id)
            .unwrap()
            .notes
            .iter()
            .find(|n| n.id == id)
            .unwrap();
        assert_eq!(note.abs_tick, 2_000);
        assert_eq!(note.duration_ticks, TICKS_PER_QUARTER * 4);
    }

    #[test]
    fn insert_note_shrinks_to_avoid_overlapping_a_later_note_on_the_same_string() {
        let mut project = BwxProject::demo();
        // Track 1's existing notes are all on string 0; place a long note on
        // string 1 first, then a short one after it on the same string —
        // the long one should shrink to stop right where the new one starts.
        let long_id = project.insert_note(1, 0, TICKS_PER_QUARTER * 4, 1, 5).unwrap();
        project.insert_note(1, 480, TICKS_PER_QUARTER, 1, 7).unwrap();
        let track = project.track(1).unwrap();
        let long_note = track.notes.iter().find(|n| n.id == long_id).unwrap();
        assert_eq!(long_note.duration_ticks, 480);
    }

    #[test]
    fn insert_note_shrinks_to_avoid_overlapping_an_earlier_note_on_the_same_string() {
        let mut project = BwxProject::demo();
        project.insert_note(1, 480, TICKS_PER_QUARTER, 1, 5).unwrap();
        // A new note starting before it, long enough to reach into it,
        // should shrink instead of overlapping.
        let new_id = project.insert_note(1, 0, TICKS_PER_QUARTER * 4, 1, 2).unwrap();
        let track = project.track(1).unwrap();
        let new_note = track.notes.iter().find(|n| n.id == new_id).unwrap();
        assert_eq!(new_note.duration_ticks, 480);
    }

    #[test]
    fn insert_note_does_not_shrink_for_notes_on_a_different_string() {
        let mut project = BwxProject::demo();
        // Track 1's note id 1 sits on string 5 at tick 0 for a full quarter
        // note; a note on a *different* string at the same tick is a chord,
        // not an overlap, and shouldn't be shrunk because of it.
        let id = project.insert_note(1, 0, TICKS_PER_QUARTER * 2, 0, 5).unwrap();
        let track = project.track(1).unwrap();
        let note = track.notes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(note.duration_ticks, TICKS_PER_QUARTER * 2);
    }

    #[test]
    fn fluid_duration_shifts_later_notes_inside_measure() {
        let mut project = BwxProject::demo();
        assert!(project.set_note_duration_fluid(1, 1, DurationChoice::Half.ticks()));
        let track = project.track(1).unwrap();
        assert_eq!(
            track.notes.iter().find(|n| n.id == 2).unwrap().abs_tick,
            1920
        );
        assert_eq!(
            track.notes.iter().find(|n| n.id == 3).unwrap().abs_tick,
            2400
        );
    }

    #[test]
    fn fluid_duration_does_not_touch_notes_in_later_bars() {
        // Regression test: `set_note_duration_fluid` used to end with a
        // `retain` that deleted every note in the whole track past the
        // *current* bar's end, not just ones actually affected by the
        // resize. A resize in bar 0 must leave every later bar untouched.
        let mut project = BwxProject::demo();
        let track_id = project.tracks[0].id;
        let bar_width = project.track(track_id).unwrap().measure_bounds_at(0).1;
        // Give the track a second bar with its own note, well past bar 0.
        assert!(project.add_bar_after(track_id, 0));
        let far_note_id = project
            .insert_note(track_id, bar_width + 480, TICKS_PER_QUARTER, 2, 9)
            .unwrap();

        assert!(project.set_note_duration_fluid(track_id, 1, DurationChoice::Half.ticks()));

        let track = project.track(track_id).unwrap();
        assert!(
            track.notes.iter().any(|note| note.id == far_note_id),
            "a note in a later bar must survive a duration change earlier in the track"
        );
    }

    #[test]
    fn rest_padding_uses_standard_chunks() {
        let rests = split_rest_gap(TICKS_PER_QUARTER, TICKS_PER_QUARTER * 4);
        assert_eq!(rests.len(), 2);
        assert_eq!(rests[0].duration_ticks, TICKS_PER_QUARTER * 2);
        assert_eq!(rests[1].duration_ticks, TICKS_PER_QUARTER);
    }

    #[test]
    fn insert_bar_before_shifts_selected_bar_and_later_notes_right() {
        let mut project = BwxProject::demo();
        let track = project.track(1).unwrap();
        let bar_width = track.measure_bounds_at(0).1; // 4/4 bar = 3840 ticks
        assert_eq!(track.bar_count, 4);

        assert!(project.insert_bar_before(1, 0));
        let track = project.track(1).unwrap();
        assert_eq!(track.bar_count, 5);
        // Every note (all originally inside bar 0) shifted right by one bar.
        assert_eq!(
            track.notes.iter().find(|n| n.id == 1).unwrap().abs_tick,
            bar_width
        );
        assert_eq!(
            track.notes.iter().find(|n| n.id == 4).unwrap().abs_tick,
            bar_width + TICKS_PER_QUARTER * 2
        );
        // The new first bar keeps the original 4/4 signature.
        assert_eq!(track.signature_at(0), TimeSignature::new(4, 4));
    }

    #[test]
    fn add_bar_after_leaves_selected_bar_untouched_and_shifts_the_rest() {
        let mut project = BwxProject::demo();
        let track = project.track(1).unwrap();
        let (bar_start, bar_end) = track.measure_bounds_at(0);
        let width = bar_end - bar_start;

        assert!(project.add_bar_after(1, 0));
        let track = project.track(1).unwrap();
        assert_eq!(track.bar_count, 5);
        // Notes inside bar 0 (ticks 0..3840) are untouched.
        assert_eq!(track.notes.iter().find(|n| n.id == 1).unwrap().abs_tick, 0);
        assert_eq!(
            track.notes.iter().find(|n| n.id == 4).unwrap().abs_tick,
            TICKS_PER_QUARTER * 2
        );
        // The new bar sits right after bar 0 and shares its signature.
        assert_eq!(track.signature_at(bar_end), TimeSignature::new(4, 4));
        assert_eq!(track.bars_end_tick(), width * 5);
    }

    #[test]
    fn delete_bar_removes_its_notes_and_shifts_the_rest_back() {
        let mut project = BwxProject::demo();
        let track = project.track(1).unwrap();
        let (_, bar_end) = track.measure_bounds_at(0);

        assert!(project.delete_bar(1, 0));
        let track = project.track(1).unwrap();
        assert_eq!(track.bar_count, 3);
        // All demo notes on this track sit inside bar 0, so they're all gone.
        assert!(track.notes.is_empty());
        assert_eq!(track.signature_at(0), TimeSignature::new(4, 4));
        assert_eq!(track.bars_end_tick(), bar_end * 3);
    }

    #[test]
    fn delete_bar_refuses_to_remove_the_last_bar() {
        let mut project = BwxProject::empty_with_track(
            "Solo",
            crate::core::project::InstrumentFamily::Stringed,
            crate::core::project::StringedVariant::AcousticGuitar,
            6,
        );
        let track_id = project.tracks[0].id;
        assert_eq!(project.track(track_id).unwrap().bar_count, 1);
        assert!(!project.delete_bar(track_id, 0));
        assert_eq!(project.track(track_id).unwrap().bar_count, 1);
    }

    #[test]
    fn time_signature_change_restarts_the_measure_grid() {
        let mut project = BwxProject::demo();
        assert!(project.set_track_time_signature(1, 2_500, TimeSignature::new(3, 4)));
        let track = project.track(1).unwrap();

        // The 4/4 measure is clipped at the new signature tick, then 3/4
        // measures begin exactly there instead of on the global tick-zero grid.
        assert_eq!(track.measure_bounds_at(2_400), (0, 2_500));
        assert_eq!(track.measure_bounds_at(2_500), (2_500, 5_380));
        assert_eq!(track.measure_bounds_at(5_380), (5_380, 8_260));
    }

    #[test]
    fn tempo_point_holds_flat_until_the_next_point() {
        let mut project = BwxProject::demo();
        assert_eq!(project.tempo_at(0), 120.0);
        assert!(project.set_tempo_point(
            TICKS_PER_QUARTER * 4,
            180.0,
            TempoTransition::Constant,
        ));
        assert_eq!(project.tempo_at(0), 120.0);
        assert_eq!(project.tempo_at(TICKS_PER_QUARTER * 2), 120.0);
        assert_eq!(project.tempo_at(TICKS_PER_QUARTER * 4), 180.0);
        assert_eq!(project.tempo_at(TICKS_PER_QUARTER * 10), 180.0);
    }

    #[test]
    fn progressive_tempo_point_ramps_toward_the_next_point() {
        let mut project = BwxProject::demo();
        assert!(project.set_tempo_point(
            0,
            120.0,
            TempoTransition::Progressive,
        ));
        assert!(project.set_tempo_point(
            TICKS_PER_QUARTER * 4,
            220.0,
            TempoTransition::Constant,
        ));
        assert_eq!(project.interpolated_tempo_at(0), 120.0);
        assert_eq!(project.interpolated_tempo_at(TICKS_PER_QUARTER * 4), 220.0);
        let midpoint = project.interpolated_tempo_at(TICKS_PER_QUARTER * 2);
        assert!((midpoint - 170.0).abs() < 0.01);
    }

    #[test]
    fn deleting_the_first_tempo_point_is_refused() {
        let mut project = BwxProject::demo();
        assert!(!project.delete_tempo_point(0));
        assert_eq!(project.tempo_points.len(), 1);
        assert!(project.set_tempo_point(
            TICKS_PER_QUARTER * 4,
            90.0,
            TempoTransition::Constant,
        ));
        assert!(project.delete_tempo_point(TICKS_PER_QUARTER * 4));
        assert_eq!(project.tempo_points.len(), 1);
    }
}
