//! Conversion between brick view and tab view.
//!
//! Brick view stores a sustained note as one `TabNote` that may span any
//! number of barlines and have any length. Tab view (Guitar Pro style) needs
//! every note to sit inside a single bar and to be writable as one note
//! value, so a brick crossing a barline, or one whose length is not a note
//! value with up to two dots, becomes a chain of tied notes. Slices cut from
//! a brick are marked `TieOrigin::BrickSplit`; only those are re-joined when
//! returning to brick view, so ties the user wrote deliberately
//! (`TieOrigin::User`) survive as separate notes.

use super::length::{LengthScale, standard_pieces_in};
use super::project::{BwxProject, InstrumentTrack, NoteEffects, TabNote, Tick, TieOrigin};

impl BwxProject {
    /// Cuts every note in every track at the barlines it crosses and into
    /// standard lengths where its length is not one. Returns whether
    /// anything changed. Idempotent.
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

    /// Cuts one note at every barline it crosses and into standard lengths.
    /// The first slice keeps the note's id and effects; each later slice is
    /// a new note tied to the one before it. Slices past the track's last
    /// bar extend `bar_count` so every slice still lands inside a real bar.
    pub fn split_note_at_barlines(&mut self, track_id: u64, note_id: u64) -> bool {
        let Some(track) = self.track(track_id) else {
            return false;
        };
        let Some(note) = track.notes.iter().find(|note| note.id == note_id) else {
            return false;
        };
        let slices = note_slices(track, note.abs_tick, note.end_tick());
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
        track.grow_bars_to_cover(last_end);
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

    /// Moves each of `note_ids` by `delta` frets on its own string. If any
    /// of them would leave the neck nothing moves and the result is `None`,
    /// so a chord keeps its shape. Otherwise it is how many notes were found.
    pub fn shift_note_frets(&mut self, track_id: u64, note_ids: &[u64], delta: i16) -> Option<usize> {
        let track = self.track(track_id)?;
        let targets: Vec<(u64, i16)> = track
            .notes
            .iter()
            .filter(|note| note_ids.contains(&note.id))
            .map(|note| (note.id, note.fret as i16 + delta))
            .collect();
        if targets
            .iter()
            .any(|&(_, fret)| fret < 0 || fret > track.fret_count as i16)
        {
            return None;
        }
        for &(id, fret) in &targets {
            self.set_note_fret(track_id, id, fret as u8);
        }
        Some(targets.len())
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

/// The pieces `[start, end)` is written as: cut at each bar boundary, then
/// each bar's share cut into standard lengths.
pub fn note_slices(track: &InstrumentTrack, start: Tick, end: Tick) -> Vec<(Tick, Tick)> {
    let mut slices = Vec::new();
    for (slice_start, slice_end) in bar_slices(track, start, end).into_iter().flat_map(|(from, to)| tuplet_slices(track, from, to)) {
        // Inside a tuplet the pieces are the plain values the parts stand
        // for, mapped back to real ticks.
        if let Some(span) = track
            .tuplet_at(slice_start)
            .filter(|span| span.is_tuplet() && slice_end <= span.end())
        {
            let mut written: Tick = 0;
            let mut cursor = slice_start;
            for length in standard_pieces_in(span.written_ticks(slice_end - slice_start), LengthScale::PLAIN) {
                written += length;
                let next = (slice_start + span.real_ticks(written)).min(slice_end);
                slices.push((cursor, next));
                cursor = next;
            }
            if let Some(last) = slices.last_mut() {
                last.1 = slice_end;
            }
            continue;
        }
        let mut cursor = slice_start;
        let scale = LengthScale::of(track.signature_at(slice_start));
        for length in standard_pieces_in(slice_end - slice_start, scale) {
            slices.push((cursor, cursor + length));
            cursor += length;
        }
    }
    slices
}

/// `[start, end)` cut where a tuplet starts or ends inside it: a note that
/// runs into a tuplet is a plain note up to it and, from there, notes of the
/// tuplet.
fn tuplet_slices(track: &InstrumentTrack, start: Tick, end: Tick) -> Vec<(Tick, Tick)> {
    let mut cuts: Vec<Tick> = track
        .tuplets
        .iter()
        .filter(|span| span.is_tuplet())
        .flat_map(|span| [span.start, span.end()])
        .filter(|&cut| start < cut && cut < end)
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut slices = Vec::new();
    let mut cursor = start;
    for cut in cuts.into_iter().chain([end]) {
        slices.push((cursor, cut));
        cursor = cut;
    }
    slices
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

    #[test]
    fn a_length_that_is_no_note_value_splits_into_tied_standard_lengths() {
        let mut project = project_with_bars(1);
        let id = project
            .insert_note(1, 0, TICKS_PER_QUARTER + TICKS_PER_QUARTER / 4, 2, 4)
            .unwrap();
        assert!(project.split_bricks_at_barlines());

        let notes = &project.tracks[0].notes;
        assert_eq!(notes.len(), 2);
        let head = notes.iter().find(|note| note.id == id).unwrap();
        assert_eq!(
            (head.abs_tick, head.duration_ticks),
            (0, TICKS_PER_QUARTER)
        );
        let tail = notes.iter().find(|note| note.id != id).unwrap();
        assert_eq!(
            (tail.abs_tick, tail.duration_ticks),
            (TICKS_PER_QUARTER, TICKS_PER_QUARTER / 4)
        );
        assert_eq!(tail.tied_from_previous, Some(TieOrigin::BrickSplit));
        assert!(!project.split_bricks_at_barlines());
    }

    #[test]
    fn merging_restores_a_brick_cut_into_standard_lengths() {
        let mut project = project_with_bars(1);
        let length = TICKS_PER_QUARTER + TICKS_PER_QUARTER / 4;
        let id = project.insert_note(1, 0, length, 2, 4).unwrap();
        project.split_bricks_at_barlines();
        assert!(project.merge_brick_ties());

        let notes = &project.tracks[0].notes;
        assert_eq!(notes.len(), 1);
        assert_eq!((notes[0].id, notes[0].duration_ticks), (id, length));
    }

    #[test]
    fn each_bar_of_a_crossing_brick_is_cut_into_standard_lengths_on_its_own() {
        let mut project = project_with_bars(2);
        // Starts a quarter and a sixteenth before the barline and runs the
        // same distance past it.
        let reach = TICKS_PER_QUARTER + TICKS_PER_QUARTER / 4;
        project.insert_note(1, BAR - reach, reach * 2, 0, 1).unwrap();
        project.split_bricks_at_barlines();
        let spans: Vec<(Tick, Tick)> = project.tracks[0]
            .notes
            .iter()
            .map(|note| (note.abs_tick, note.duration_ticks))
            .collect();
        assert_eq!(
            spans,
            vec![
                (BAR - reach, TICKS_PER_QUARTER),
                (BAR - TICKS_PER_QUARTER / 4, TICKS_PER_QUARTER / 4),
                (BAR, TICKS_PER_QUARTER),
                (BAR + TICKS_PER_QUARTER, TICKS_PER_QUARTER / 4),
            ]
        );
    }

    #[test]
    fn shifting_frets_moves_every_note_or_none() {
        let mut project = project_with_bars(1);
        let low = project.insert_note(1, 0, TICKS_PER_QUARTER, 0, 0).unwrap();
        let high = project.insert_note(1, 0, TICKS_PER_QUARTER, 1, 5).unwrap();

        assert_eq!(project.shift_note_frets(1, &[low, high], 1), Some(2));
        let frets: Vec<u8> = project.tracks[0].notes.iter().map(|note| note.fret).collect();
        assert_eq!(frets, vec![1, 6]);

        // Going down would push the first note below the open string.
        assert_eq!(project.shift_note_frets(1, &[low, high], -2), None);
        let frets: Vec<u8> = project.tracks[0].notes.iter().map(|note| note.fret).collect();
        assert_eq!(frets, vec![1, 6]);
    }

    #[test]
    fn shifting_frets_stops_at_the_top_of_the_neck() {
        let mut project = project_with_bars(1);
        let top = project.tracks[0].fret_count;
        let id = project.insert_note(1, 0, TICKS_PER_QUARTER, 0, top).unwrap();
        assert_eq!(project.shift_note_frets(1, &[id], 1), None);
        assert_eq!(project.shift_note_frets(1, &[], 1), Some(0));
    }

    #[test]
    fn a_note_filling_an_irrational_bar_stays_whole_and_longer_ones_cut_at_its_barline() {
        let mut project = project_with_bars(3);
        project.set_track_time_signature(1, 0, crate::core::TimeSignature::new(4, 6));
        let bar = crate::core::TimeSignature::new(4, 6).measure_ticks();
        assert_eq!(bar, 2560);
        let id = project.insert_note(1, 0, bar, 0, 5).unwrap();
        assert!(!project.split_bricks_at_barlines());
        assert_eq!(project.tracks[0].notes.len(), 1);

        let longer = project.insert_note(1, bar, bar + 640, 1, 3).unwrap();
        assert!(project.split_bricks_at_barlines());
        let pieces: Vec<Tick> = project.tracks[0]
            .notes
            .iter()
            .filter(|note| note.string_index == 1)
            .map(|note| note.duration_ticks)
            .collect();
        assert_eq!(pieces.iter().sum::<Tick>(), bar + 640);
        assert!(pieces.contains(&bar));
        let _ = (id, longer);
    }

    #[test]
    fn a_five_twelfths_bar_splits_a_full_length_note_into_triplet_values() {
        let mut project = project_with_bars(1);
        project.set_track_time_signature(1, 0, crate::core::TimeSignature::new(5, 12));
        let _ = project.insert_note(1, 0, 1600, 0, 5).unwrap();
        assert!(project.split_bricks_at_barlines());
        let mut lengths: Vec<Tick> = project.tracks[0].notes.iter().map(|note| note.duration_ticks).collect();
        lengths.sort_unstable();
        assert_eq!(lengths, [320, 1280]);
    }
}
