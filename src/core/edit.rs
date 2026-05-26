use super::project::{BwxProject, NoteEffects, RestNode, TICKS_PER_QUARTER, TabNote, Tick};

impl BwxProject {
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
        let bounded_fret = fret.min(track.fret_count);
        track.notes.push(TabNote {
            id,
            abs_tick: abs_tick.max(0),
            duration_ticks: duration_ticks.max(TICKS_PER_QUARTER / 32),
            string_index: string_index.min(track.tuning.len().saturating_sub(1)),
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

        track.notes.retain(|note| note.abs_tick < measure_end);
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
    fn rest_padding_uses_standard_chunks() {
        let rests = split_rest_gap(TICKS_PER_QUARTER, TICKS_PER_QUARTER * 4);
        assert_eq!(rests.len(), 2);
        assert_eq!(rests[0].duration_ticks, TICKS_PER_QUARTER * 2);
        assert_eq!(rests[1].duration_ticks, TICKS_PER_QUARTER);
    }
}
