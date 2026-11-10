//! Conversion between brick view and tab view.
//!
//! Brick view stores a sustained note as one `TabNote` that may span any
//! number of barlines. Tab view (Guitar Pro style) needs every note to sit
//! inside a single bar, so a brick crossing a barline becomes a chain of
//! tied notes. Slices cut from a brick are marked `TieOrigin::BrickSplit`;
//! only those are re-joined when returning to brick view, so ties the user
//! wrote deliberately (`TieOrigin::User`) survive as separate notes.

use super::project::{BwxProject, InstrumentTrack, NoteEffects, TabNote, Tick, TieOrigin};

impl BwxProject {
    /// Cuts every note in every track at the barlines it crosses. Returns
    /// whether anything changed. Idempotent.
    pub fn split_bricks_at_barlines(&mut self) -> bool {
        let mut changed = false;
        let track_ids: Vec<u64> = self.tracks.iter().map(|track| track.id).collect();
        for track_id in track_ids {
            let note_ids: Vec<u64> = self
                .track(track_id)
                .map(|track| track.notes.iter().map(|note| note.id).collect())
                .unwrap_or_default();
            for note_id in note_ids {
                changed |= self.split_note_at_barlines(track_id, note_id);
            }
        }
        changed
    }

    /// Cuts one note at every barline it crosses. The first slice keeps the
    /// note's id and effects; each later slice is a new note tied to the
    /// one before it. Slices past the track's last bar extend `bar_count`
    /// so every slice still lands inside a real bar.
    pub fn split_note_at_barlines(&mut self, track_id: u64, note_id: u64) -> bool {
        let Some(track) = self.track(track_id) else {
            return false;
        };
        let Some(note) = track.notes.iter().find(|note| note.id == note_id) else {
            return false;
        };
        let slices = bar_slices(track, note.abs_tick, note.end_tick());
        if slices.len() < 2 {
            return false;
        }

        let template = note.clone();
        let continuation_ids: Vec<u64> =
            (1..slices.len()).map(|_| self.allocate_note_id()).collect();
        let Some(track) = self.track_mut(track_id) else {
            return false;
        };

        if let Some(first) = track.notes.iter_mut().find(|note| note.id == note_id) {
            first.duration_ticks = slices[0].1 - slices[0].0;
        }
        for (id, &(start, end)) in continuation_ids.into_iter().zip(&slices[1..]) {
            track.notes.push(TabNote {
                id,
                abs_tick: start,
                duration_ticks: end - start,
                string_index: template.string_index,
                fret: template.fret,
                velocity: template.velocity,
                // Attack articulations (accent, slide, hammer-on, ...)
                // belong to the onset only.
                effects: NoteEffects::default(),
                tied_from_previous: Some(TieOrigin::BrickSplit),
            });
        }

        let last_end = slices.last().map(|slice| slice.1).unwrap_or(0);
        while track.bars_end_tick() < last_end {
            track.bar_count += 1;
        }
        track.sort_notes();
        true
    }

    /// Joins every `BrickSplit` continuation back into the note before it,
    /// restoring the original single brick. `User` ties are left alone.
    /// Returns whether anything changed. Idempotent.
    pub fn merge_brick_ties(&mut self) -> bool {
        let mut changed = false;
        for track in &mut self.tracks {
            while let Some((predecessor_id, continuation_id, extra_ticks)) =
                find_brick_split_pair(track)
            {
                if let Some(predecessor) = track
                    .notes
                    .iter_mut()
                    .find(|note| note.id == predecessor_id)
                {
                    predecessor.duration_ticks += extra_ticks;
                }
                track.notes.retain(|note| note.id != continuation_id);
                changed = true;
            }
            track.sort_notes();
        }
        changed
    }

    /// Toggles a user tie on a note: removes an existing tie, or ties the
    /// note to the one directly before it on the same string. Refuses (and
    /// returns `false`) when there is no adjacent note with the same fret to
    /// tie to.
    pub fn toggle_user_tie(&mut self, track_id: u64, note_id: u64) -> bool {
        let Some(track) = self.track_mut(track_id) else {
            return false;
        };
        let Some(index) = track.notes.iter().position(|note| note.id == note_id) else {
            return false;
        };
        if track.notes[index].tied_from_previous.is_some() {
            track.notes[index].tied_from_previous = None;
            return true;
        }
        let candidate = &track.notes[index];
        let has_predecessor = track.notes.iter().any(|other| {
            other.id != candidate.id
                && other.string_index == candidate.string_index
                && other.fret == candidate.fret
                && other.end_tick() == candidate.abs_tick
        });
        if !has_predecessor {
            return false;
        }
        track.notes[index].tied_from_previous = Some(TieOrigin::User);
        true
    }
}

impl BwxProject {
    /// Sets a note's fret, carrying the change through its whole tie chain
    /// (earlier and later notes it is tied with) so the chain keeps sounding
    /// as one pitch. Frets are bounded by the track's `fret_count`. Returns
    /// whether any fret actually changed.
    pub fn set_note_fret(&mut self, track_id: u64, note_id: u64, fret: u8) -> bool {
        let Some(track) = self.track_mut(track_id) else {
            return false;
        };
        let fret = fret.min(track.fret_count);
        let chain = tie_chain_ids(track, note_id);
        let mut changed = false;
        for note in track
            .notes
            .iter_mut()
            .filter(|note| chain.contains(&note.id))
        {
            if note.fret != fret {
                note.fret = fret;
                changed = true;
            }
        }
        changed
    }
}

/// Ids of every note tied together with `note_id` (including itself), found
/// by adjacency on the same string and ignoring fret, so a chain whose
/// frets already disagree is still treated as one unit.
fn tie_chain_ids(track: &InstrumentTrack, note_id: u64) -> Vec<u64> {
    let Some(start) = track.notes.iter().find(|note| note.id == note_id) else {
        return Vec::new();
    };
    let mut chain = vec![start.id];

    let mut current = start;
    while chain.len() <= track.notes.len() {
        let previous = current.tied_from_previous.and_then(|_| {
            track.notes.iter().find(|other| {
                other.string_index == current.string_index && other.end_tick() == current.abs_tick
            })
        });
        match previous {
            Some(previous) if !chain.contains(&previous.id) => {
                chain.push(previous.id);
                current = previous;
            }
            _ => break,
        }
    }

    let mut current = start;
    while chain.len() <= track.notes.len() {
        let next = track.notes.iter().find(|other| {
            other.tied_from_previous.is_some()
                && other.string_index == current.string_index
                && other.abs_tick == current.end_tick()
        });
        match next {
            Some(next) if !chain.contains(&next.id) => {
                chain.push(next.id);
                current = next;
            }
            _ => break,
        }
    }
    chain
}

/// `[start, end)` pieces of `[start, end)` cut at each bar boundary.
fn bar_slices(track: &InstrumentTrack, start: Tick, end: Tick) -> Vec<(Tick, Tick)> {
    let mut slices = Vec::new();
    let mut cursor = start;
    while cursor < end {
        let (_, bar_end) = track.measure_bounds_at(cursor);
        let slice_end = end.min(bar_end);
        slices.push((cursor, slice_end));
        cursor = slice_end;
    }
    slices
}

/// First `(predecessor id, continuation id, continuation duration)` where
/// the continuation is a `BrickSplit` tie with a valid predecessor.
fn find_brick_split_pair(track: &InstrumentTrack) -> Option<(u64, u64, Tick)> {
    track.notes.iter().find_map(|note| {
        if note.tied_from_previous != Some(TieOrigin::BrickSplit) {
            return None;
        }
        track
            .tie_predecessor(note)
            .map(|predecessor| (predecessor.id, note.id, note.duration_ticks))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{InstrumentFamily, StringedVariant, TICKS_PER_QUARTER};

    const BAR: Tick = TICKS_PER_QUARTER * 4;

    fn project_with_bars(bars: u32) -> BwxProject {
        let mut project = BwxProject::empty_with_track(
            "t",
            InstrumentFamily::Stringed,
            StringedVariant::AcousticGuitar,
            6,
        );
        project.tracks[0].bar_count = bars;
        project
    }

    #[test]
    fn brick_crossing_one_barline_splits_into_two_tied_notes() {
        let mut project = project_with_bars(2);
        let id = project
            .insert_note(1, BAR - TICKS_PER_QUARTER, TICKS_PER_QUARTER * 2, 0, 5)
            .unwrap();
        assert!(project.split_bricks_at_barlines());

        let notes = &project.tracks[0].notes;
        assert_eq!(notes.len(), 2);
        let head = notes.iter().find(|note| note.id == id).unwrap();
        assert_eq!(
            (head.abs_tick, head.duration_ticks),
            (BAR - TICKS_PER_QUARTER, TICKS_PER_QUARTER)
        );
        assert_eq!(head.tied_from_previous, None);
        let tail = notes.iter().find(|note| note.id != id).unwrap();
        assert_eq!(
            (tail.abs_tick, tail.duration_ticks),
            (BAR, TICKS_PER_QUARTER)
        );
        assert_eq!(tail.tied_from_previous, Some(TieOrigin::BrickSplit));
        assert_eq!(tail.fret, 5);
    }

    #[test]
    fn brick_spanning_several_bars_splits_at_every_barline() {
        let mut project = project_with_bars(3);
        project.insert_note(1, BAR / 2, BAR * 2, 1, 3).unwrap();
        project.split_bricks_at_barlines();
        let durations: Vec<Tick> = project.tracks[0]
            .notes
            .iter()
            .map(|note| note.duration_ticks)
            .collect();
        assert_eq!(durations, vec![BAR / 2, BAR, BAR / 2]);
    }

    #[test]
    fn split_follows_polymetric_bar_widths() {
        let mut project = project_with_bars(3);
        project.set_track_time_signature(1, 0, crate::core::TimeSignature::new(3, 4));
        project.insert_note(1, 0, BAR, 0, 0).unwrap();
        project.split_bricks_at_barlines();
        let durations: Vec<Tick> = project.tracks[0]
            .notes
            .iter()
            .map(|note| note.duration_ticks)
            .collect();
        assert_eq!(durations, vec![TICKS_PER_QUARTER * 3, TICKS_PER_QUARTER]);
    }

    #[test]
    fn splitting_past_the_last_bar_adds_bars() {
        let mut project = project_with_bars(1);
        project.insert_note(1, 0, BAR * 2, 0, 0).unwrap();
        project.split_bricks_at_barlines();
        assert_eq!(project.tracks[0].bar_count, 2);
    }

    #[test]
    fn splitting_is_idempotent() {
        let mut project = project_with_bars(2);
        project.insert_note(1, BAR - TICKS_PER_QUARTER, TICKS_PER_QUARTER * 2, 0, 5);
        assert!(project.split_bricks_at_barlines());
        let after_first = project.clone();
        assert!(!project.split_bricks_at_barlines());
        assert_eq!(project, after_first);
    }

    #[test]
    fn merging_restores_the_original_brick() {
        let mut project = project_with_bars(3);
        let id = project.insert_note(1, BAR / 2, BAR * 2, 0, 7).unwrap();
        let original = project.tracks[0].notes[0].clone();
        project.split_bricks_at_barlines();
        assert!(project.merge_brick_ties());

        let notes = &project.tracks[0].notes;
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, id);
        assert_eq!(notes[0], original);
        assert!(!project.merge_brick_ties());
    }

    #[test]
    fn user_ties_stay_separate_when_merging() {
        let mut project = project_with_bars(2);
        project.insert_note(1, BAR - TICKS_PER_QUARTER, TICKS_PER_QUARTER, 0, 5);
        let second = project
            .insert_note(1, BAR, TICKS_PER_QUARTER, 0, 5)
            .unwrap();
        assert!(project.toggle_user_tie(1, second));

        project.split_bricks_at_barlines();
        project.merge_brick_ties();

        let notes = &project.tracks[0].notes;
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[1].tied_from_previous, Some(TieOrigin::User));
    }

    #[test]
    fn only_the_brick_slice_merges_when_a_user_tie_sits_in_the_chain() {
        let mut project = project_with_bars(3);
        // A brick crossing bar 1 -> 2, then a user-tied note right after it.
        project.insert_note(1, BAR - TICKS_PER_QUARTER, TICKS_PER_QUARTER * 2, 0, 5);
        let user_tied = project
            .insert_note(1, BAR + TICKS_PER_QUARTER, TICKS_PER_QUARTER, 0, 5)
            .unwrap();
        assert!(project.toggle_user_tie(1, user_tied));

        project.split_bricks_at_barlines();
        project.merge_brick_ties();

        let notes = &project.tracks[0].notes;
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].duration_ticks, TICKS_PER_QUARTER * 2);
        assert_eq!(notes[1].id, user_tied);
    }

    #[test]
    fn a_slice_whose_predecessor_was_deleted_is_not_merged() {
        let mut project = project_with_bars(2);
        let head = project
            .insert_note(1, BAR - TICKS_PER_QUARTER, TICKS_PER_QUARTER * 2, 0, 5)
            .unwrap();
        project.split_bricks_at_barlines();
        project.delete_note(1, head);
        project.merge_brick_ties();
        assert_eq!(project.tracks[0].notes.len(), 1);
        assert_eq!(project.tracks[0].notes[0].duration_ticks, TICKS_PER_QUARTER);
        assert!(!project.tracks[0].is_tied_continuation(&project.tracks[0].notes[0]));
    }

    #[test]
    fn toggle_user_tie_needs_an_adjacent_note_with_the_same_fret() {
        let mut project = project_with_bars(1);
        project.insert_note(1, 0, TICKS_PER_QUARTER, 0, 5);
        let different_fret = project
            .insert_note(1, TICKS_PER_QUARTER, TICKS_PER_QUARTER, 0, 7)
            .unwrap();
        let gap = project
            .insert_note(1, TICKS_PER_QUARTER * 3, TICKS_PER_QUARTER, 0, 5)
            .unwrap();
        assert!(!project.toggle_user_tie(1, different_fret));
        assert!(!project.toggle_user_tie(1, gap));
    }

    #[test]
    fn changing_one_fret_changes_the_whole_tie_chain() {
        let mut project = project_with_bars(3);
        let id = project.insert_note(1, BAR / 2, BAR * 2, 0, 5).unwrap();
        project.split_bricks_at_barlines();
        let middle = project.tracks[0]
            .notes
            .iter()
            .find(|note| note.abs_tick == BAR)
            .unwrap()
            .id;

        assert!(project.set_note_fret(1, middle, 9));
        assert!(project.tracks[0].notes.iter().all(|note| note.fret == 9));
        assert!(project.merge_brick_ties());
        assert_eq!(project.tracks[0].notes.len(), 1);
        assert_eq!(project.tracks[0].notes[0].id, id);
    }

    #[test]
    fn toggle_user_tie_twice_removes_the_tie() {
        let mut project = project_with_bars(1);
        project.insert_note(1, 0, TICKS_PER_QUARTER, 0, 5);
        let second = project
            .insert_note(1, TICKS_PER_QUARTER, TICKS_PER_QUARTER, 0, 5)
            .unwrap();
        assert!(project.toggle_user_tie(1, second));
        assert!(project.toggle_user_tie(1, second));
        assert_eq!(project.tracks[0].notes[1].tied_from_previous, None);
    }
}
