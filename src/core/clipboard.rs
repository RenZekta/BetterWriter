//! Copying notes out of a track and pasting them back in.

use super::project::{BwxProject, InstrumentTrack, NoteEffects, TabNote, Tick, TieOrigin};
use super::smf::SmfNote;

/// One copied note, positioned relative to the earliest copied onset.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ClipNote {
    offset: Tick,
    duration_ticks: Tick,
    string_index: usize,
    fret: u8,
    /// The MIDI key the note sounds at in the track it came from.
    key: u8,
    velocity: u8,
    effects: NoteEffects,
    tied_from_previous: Option<TieOrigin>,
}

/// A set of copied notes, independent of where they came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteClip {
    notes: Vec<ClipNote>,
    /// Strings of the track the notes came from.
    strings: usize,
}

/// What a paste did, so the editor can select and show the new notes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteOutcome {
    /// Tick the first pasted note landed on.
    pub start_tick: Tick,
    /// Where the last pasted note stops.
    pub end_tick: Tick,
    pub note_ids: Vec<u64>,
    pub bars_added: u32,
}

impl NoteClip {
    /// Copies the notes of `track` named in `ids`. `None` when none of them
    /// exist.
    pub fn from_track(track: &InstrumentTrack, ids: &[u64]) -> Option<NoteClip> {
        let chosen: Vec<&TabNote> = track
            .notes
            .iter()
            .filter(|note| ids.contains(&note.id))
            .collect();
        let origin = chosen.iter().map(|note| note.abs_tick).min()?;
        let notes = chosen
            .iter()
            .map(|note| ClipNote {
                offset: note.abs_tick - origin,
                duration_ticks: note.duration_ticks,
                string_index: note.string_index,
                fret: note.fret,
                key: track.midi_key_for(note),
                velocity: note.velocity,
                effects: note.effects.clone(),
                tied_from_previous: note.tied_from_previous,
            })
            .collect();
        Some(NoteClip {
            notes,
            strings: track.tuning.len(),
        })
    }

    pub fn len(&self) -> usize {
        self.notes.len()
    }

    /// A note continues an earlier one when it is tied to a note on the same
    /// string and fret that ends where it starts.
    fn tie_head(&self, index: usize) -> Option<usize> {
        let note = &self.notes[index];
        note.tied_from_previous?;
        self.notes.iter().position(|other| {
            other.string_index == note.string_index
                && other.fret == note.fret
                && other.offset + other.duration_ticks == note.offset
        })
    }

    /// The notes as they sound, with tied notes merged into one.
    pub fn midi_notes(&self) -> Vec<SmfNote> {
        let mut merged: Vec<SmfNote> = Vec::new();
        let mut owner: Vec<usize> = vec![0; self.notes.len()];
        let mut order: Vec<usize> = (0..self.notes.len()).collect();
        order.sort_by_key(|&index| self.notes[index].offset);
        for index in order {
            let note = &self.notes[index];
            if let Some(head) = self.tie_head(index) {
                let target = owner[head];
                owner[index] = target;
                merged[target].duration += note.duration_ticks;
                continue;
            }
            owner[index] = merged.len();
            merged.push(SmfNote {
                start: note.offset,
                duration: note.duration_ticks,
                key: note.key,
                velocity: note.velocity,
            });
        }
        merged
    }

    /// The notes as a plain text tab, top line the highest string. Columns
    /// follow the order of the onsets, not their durations.
    pub fn ascii_tab(&self) -> String {
        let mut onsets: Vec<Tick> = self.notes.iter().map(|note| note.offset).collect();
        onsets.sort_unstable();
        onsets.dedup();
        let rows = self
            .strings
            .max(self.notes.iter().map(|note| note.string_index + 1).max().unwrap_or(0));
        let mut lines: Vec<String> = vec![String::from("|-"); rows];
        for onset in onsets {
            let cells: Vec<Option<String>> = (0..rows)
                .map(|string| {
                    self.notes
                        .iter()
                        .find(|note| note.offset == onset && note.string_index == string)
                        .map(|note| note.fret.to_string())
                })
                .collect();
            let width = cells.iter().flatten().map(String::len).max().unwrap_or(1);
            for (string, cell) in cells.into_iter().enumerate() {
                let text = cell.unwrap_or_default();
                lines[string].push_str(&format!("{text:-<width$}-"));
            }
        }
        lines
            .iter()
            .rev()
            .map(|line| format!("{line}|"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Notes from a MIDI clip placed on the strings of `track`: each takes
    /// the free string that reaches its key at the lowest fret. Notes no
    /// string can play, or that find every fitting string busy, are left out.
    /// `None` when nothing fits.
    pub fn from_midi(notes: &[SmfNote], track: &InstrumentTrack) -> Option<NoteClip> {
        let origin = notes.iter().map(|note| note.start).min()?;
        let mut sorted: Vec<&SmfNote> = notes.iter().collect();
        sorted.sort_by_key(|note| (note.start, note.key));
        let mut placed: Vec<ClipNote> = Vec::new();
        for note in sorted {
            let offset = note.start - origin;
            let free = |string: usize| {
                !placed.iter().any(|other| {
                    other.string_index == string
                        && other.offset < offset + note.duration
                        && other.offset + other.duration_ticks > offset
                })
            };
            let choice = track
                .tuning
                .iter()
                .enumerate()
                .filter_map(|(string, &open)| {
                    let fret = i16::from(note.key) - i16::from(open);
                    (0..=i16::from(track.fret_count))
                        .contains(&fret)
                        .then_some((fret as u8, string))
                })
                .filter(|&(_, string)| free(string))
                .min();
            if let Some((fret, string_index)) = choice {
                placed.push(ClipNote {
                    offset,
                    duration_ticks: note.duration.max(1),
                    string_index,
                    fret,
                    key: note.key,
                    velocity: note.velocity.clamp(1, 127),
                    effects: NoteEffects::default(),
                    tied_from_previous: None,
                });
            }
        }
        (!placed.is_empty()).then_some(NoteClip {
            notes: placed,
            strings: track.tuning.len(),
        })
    }
}

impl BwxProject {
    /// Pastes `clip` into a track with its first note on `at_tick`.
    ///
    /// Pasted notes replace what they land on: on the same string, a note
    /// that starts inside a pasted note is removed and one sounding across
    /// its start is cut short there. Notes on other strings are untouched.
    /// Copied notes on strings the track doesn't have are skipped.
    ///
    /// When `at_tick` is at or past the end of the track's last bar, one bar
    /// is appended (it continues the last bar's time signature) and the notes
    /// go to its start instead. Bars are also appended as needed so every
    /// pasted note ends inside the track.
    ///
    /// `None` when nothing could be pasted.
    pub fn paste_notes(
        &mut self,
        track_id: u64,
        clip: &NoteClip,
        at_tick: Tick,
    ) -> Option<PasteOutcome> {
        let strings = self.track(track_id)?.tuning.len();
        let usable: Vec<&ClipNote> = clip
            .notes
            .iter()
            .filter(|note| note.string_index < strings)
            .collect();
        if usable.is_empty() {
            return None;
        }

        let mut bars_added = 0;
        let mut start = at_tick.max(0);
        let track_end = self.track(track_id)?.bars_end_tick();
        if start >= track_end {
            if !self.add_bar_after(track_id, (track_end - 1).max(0)) {
                return None;
            }
            start = track_end;
            bars_added += 1;
        }

        let ids: Vec<u64> = usable.iter().map(|_| self.allocate_note_id()).collect();
        let track = self.track_mut(track_id)?;

        for note in &usable {
            let note_start = start + note.offset;
            let note_end = note_start + note.duration_ticks;
            track.notes.retain(|existing| {
                !(existing.string_index == note.string_index
                    && existing.abs_tick >= note_start
                    && existing.abs_tick < note_end)
            });
            for existing in track.notes.iter_mut() {
                if existing.string_index == note.string_index
                    && existing.abs_tick < note_start
                    && existing.end_tick() > note_start
                {
                    existing.duration_ticks = note_start - existing.abs_tick;
                }
            }
        }

        let max_fret = track.fret_count;
        let mut last_end = start;
        for (note, id) in usable.iter().zip(&ids) {
            // A tie only survives when the note it continues was copied too.
            let tie_kept = note.tied_from_previous.is_some()
                && usable.iter().any(|other| {
                    other.string_index == note.string_index
                        && other.fret == note.fret
                        && other.offset + other.duration_ticks == note.offset
                });
            let abs_tick = start + note.offset;
            last_end = last_end.max(abs_tick + note.duration_ticks);
            track.notes.push(TabNote {
                id: *id,
                abs_tick,
                duration_ticks: note.duration_ticks,
                string_index: note.string_index,
                fret: note.fret.min(max_fret),
                velocity: note.velocity,
                effects: note.effects.clone(),
                tied_from_previous: if tie_kept {
                    note.tied_from_previous
                } else {
                    None
                },
            });
        }
        while track.bars_end_tick() < last_end {
            track.bar_count += 1;
            bars_added += 1;
        }
        track.sort_notes();

        Some(PasteOutcome {
            start_tick: start,
            end_tick: last_end,
            note_ids: ids,
            bars_added,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::project::{TICKS_PER_QUARTER, TimeSignature};

    const Q: Tick = TICKS_PER_QUARTER;

    fn project_with(notes: &[(Tick, Tick, usize, u8)]) -> (BwxProject, u64) {
        let mut project = BwxProject::demo();
        let track_id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        project.tracks[0].bar_count = 2;
        for &(tick, duration, string, fret) in notes {
            project
                .insert_note(track_id, tick, duration, string, fret)
                .unwrap();
        }
        (project, track_id)
    }

    fn all_ids(project: &BwxProject, track_id: u64) -> Vec<u64> {
        project
            .track(track_id)
            .unwrap()
            .notes
            .iter()
            .map(|note| note.id)
            .collect()
    }

    fn clip_of_all(project: &BwxProject, track_id: u64) -> NoteClip {
        let track = project.track(track_id).unwrap();
        NoteClip::from_track(track, &all_ids(project, track_id)).unwrap()
    }

    fn onsets(project: &BwxProject, track_id: u64) -> Vec<(Tick, usize, u8)> {
        project
            .track(track_id)
            .unwrap()
            .notes
            .iter()
            .map(|note| (note.abs_tick, note.string_index, note.fret))
            .collect()
    }

    #[test]
    fn a_clip_keeps_positions_relative_to_its_first_note() {
        let (project, track_id) = project_with(&[(Q, Q, 0, 1), (Q * 2, Q, 1, 2)]);
        let track = project.track(track_id).unwrap();
        assert_eq!(NoteClip::from_track(track, &[]), None);
        assert_eq!(NoteClip::from_track(track, &[9999]), None);
        let clip = NoteClip::from_track(track, &all_ids(&project, track_id)).unwrap();
        assert_eq!(clip.len(), 2);
        assert_eq!(clip.notes.iter().map(|n| n.offset).collect::<Vec<_>>(), [0, Q]);
    }

    #[test]
    fn pasting_places_the_first_note_on_the_target_tick() {
        let (mut project, track_id) = project_with(&[(0, Q, 0, 1), (Q, Q, 1, 2)]);
        let clip = clip_of_all(&project, track_id);
        let outcome = project.paste_notes(track_id, &clip, Q * 4).unwrap();
        assert_eq!(outcome.start_tick, Q * 4);
        assert_eq!(outcome.bars_added, 0);
        assert_eq!(outcome.note_ids.len(), 2);
        assert_eq!(
            onsets(&project, track_id),
            [(0, 0, 1), (Q, 1, 2), (Q * 4, 0, 1), (Q * 5, 1, 2)]
        );
    }

    #[test]
    fn the_outcome_ends_where_the_last_pasted_note_stops() {
        let (mut project, track_id) = project_with(&[(0, Q, 0, 1), (Q, Q * 2, 1, 2)]);
        let clip = clip_of_all(&project, track_id);
        let outcome = project.paste_notes(track_id, &clip, Q * 4).unwrap();
        assert_eq!(outcome.start_tick, Q * 4);
        assert_eq!(outcome.end_tick, Q * 7);

        let single = NoteClip::from_track(
            project.track(track_id).unwrap(),
            &[project.track(track_id).unwrap().notes[0].id],
        )
        .unwrap();
        let outcome = project.paste_notes(track_id, &single, Q * 8).unwrap();
        assert_eq!(outcome.end_tick, Q * 9);
    }

    #[test]
    fn pasted_notes_get_fresh_ids() {
        let (mut project, track_id) = project_with(&[(0, Q, 0, 1)]);
        let clip = clip_of_all(&project, track_id);
        let before = all_ids(&project, track_id);
        let outcome = project.paste_notes(track_id, &clip, Q * 4).unwrap();
        assert!(outcome.note_ids.iter().all(|id| !before.contains(id)));
    }

    #[test]
    fn pasting_replaces_notes_it_lands_on_and_cuts_ones_sounding_across() {
        let (mut project, track_id) = project_with(&[(0, Q * 2, 0, 5), (Q, Q, 1, 7)]);
        let clip = NoteClip::from_track(
            project.track(track_id).unwrap(),
            &[project.track(track_id).unwrap().notes[1].id],
        )
        .unwrap();
        let outcome = project.paste_notes(track_id, &clip, Q).unwrap();
        // The copied note was on string 1, so string 0 is untouched.
        assert_eq!(outcome.start_tick, Q);
        let track = project.track(track_id).unwrap();
        assert_eq!(track.notes.len(), 2);

        // Paste a string-0 note across the middle of the long string-0 note.
        let (mut project, track_id) = project_with(&[(0, Q * 2, 0, 5), (Q * 2, Q, 0, 9)]);
        let source = project.track(track_id).unwrap().notes[1].id;
        let clip = NoteClip::from_track(project.track(track_id).unwrap(), &[source]).unwrap();
        project.paste_notes(track_id, &clip, Q).unwrap();
        let track = project.track(track_id).unwrap();
        let first = track.notes.iter().find(|note| note.abs_tick == 0).unwrap();
        assert_eq!(first.duration_ticks, Q, "cut where the paste starts");
        assert!(
            track
                .notes
                .iter()
                .any(|note| note.abs_tick == Q && note.fret == 9)
        );
    }

    #[test]
    fn a_paste_removes_same_string_notes_that_start_inside_it() {
        let (mut project, track_id) = project_with(&[(0, Q * 2, 0, 5), (Q * 4, Q, 0, 1)]);
        let source = project.track(track_id).unwrap().notes[0].id;
        let clip = NoteClip::from_track(project.track(track_id).unwrap(), &[source]).unwrap();
        // Landing at Q*3 covers [Q*3, Q*5): the note at Q*4 is replaced.
        project.paste_notes(track_id, &clip, Q * 3).unwrap();
        let track = project.track(track_id).unwrap();
        assert!(!track.notes.iter().any(|note| note.fret == 1));
        assert!(track.notes.iter().any(|note| note.abs_tick == Q * 3));
    }

    #[test]
    fn pasting_past_the_last_bar_appends_a_bar_and_pastes_at_its_start() {
        let (mut project, track_id) = project_with(&[(0, Q, 0, 1), (Q, Q, 1, 2)]);
        let clip = clip_of_all(&project, track_id);
        let end = project.track(track_id).unwrap().bars_end_tick();
        let outcome = project.paste_notes(track_id, &clip, end + Q * 10).unwrap();
        assert_eq!(outcome.start_tick, end, "at the new bar's start, not the cursor");
        assert_eq!(outcome.bars_added, 1);
        let track = project.track(track_id).unwrap();
        assert_eq!(track.bar_count, 3);
        assert!(track.bars_end_tick() > end);
    }

    #[test]
    fn the_appended_bar_has_the_last_bars_time_signature() {
        let (mut project, track_id) = project_with(&[(0, Q, 0, 1)]);
        project.set_track_time_signature(track_id, 0, TimeSignature::new(3, 4));
        let clip = clip_of_all(&project, track_id);
        let end = project.track(track_id).unwrap().bars_end_tick();
        project.paste_notes(track_id, &clip, end).unwrap();
        let track = project.track(track_id).unwrap();
        assert_eq!(track.measure_bounds_at(end), (end, end + Q * 3));
        assert_eq!(track.bars_end_tick(), end + Q * 3);
    }

    #[test]
    fn bars_are_added_so_a_long_paste_still_fits() {
        let (mut project, track_id) = project_with(&[(0, Q * 4, 0, 1), (Q * 4, Q * 4, 1, 2)]);
        let clip = clip_of_all(&project, track_id);
        let end = project.track(track_id).unwrap().bars_end_tick();
        let outcome = project.paste_notes(track_id, &clip, end).unwrap();
        assert_eq!(outcome.bars_added, 2);
        let track = project.track(track_id).unwrap();
        let last_end = track.notes.iter().map(TabNote::end_tick).max().unwrap();
        assert!(track.bars_end_tick() >= last_end);
    }

    #[test]
    fn notes_on_strings_the_track_lacks_are_skipped() {
        let (mut project, track_id) = project_with(&[(0, Q, 5, 3), (Q, Q, 0, 4)]);
        let clip = clip_of_all(&project, track_id);
        project.track_mut(track_id).unwrap().tuning.truncate(4);
        project.track_mut(track_id).unwrap().notes.clear();
        let outcome = project.paste_notes(track_id, &clip, 0).unwrap();
        assert_eq!(outcome.note_ids.len(), 1);
        let track = project.track(track_id).unwrap();
        assert_eq!(track.notes.len(), 1);
        assert_eq!(track.notes[0].string_index, 0);
        // Only copied notes that don't fit anywhere: nothing to paste.
        let only_high = NoteClip {
            notes: vec![clip.notes[0].clone()],
            strings: clip.strings,
        };
        assert_eq!(project.paste_notes(track_id, &only_high, 0), None);
    }

    #[test]
    fn ties_survive_only_with_their_copied_predecessor() {
        let (mut project, track_id) = project_with(&[(0, Q, 0, 3), (Q, Q, 0, 3)]);
        project.track_mut(track_id).unwrap().notes[1].tied_from_previous = Some(TieOrigin::User);
        let tail = project.track(track_id).unwrap().notes[1].id;

        let whole = clip_of_all(&project, track_id);
        let outcome = project.paste_notes(track_id, &whole, Q * 4).unwrap();
        let pasted_tail = outcome.note_ids[1];
        let track = project.track(track_id).unwrap();
        let note = track.notes.iter().find(|n| n.id == pasted_tail).unwrap();
        assert_eq!(note.tied_from_previous, Some(TieOrigin::User));

        let alone = NoteClip::from_track(project.track(track_id).unwrap(), &[tail]).unwrap();
        let outcome = project.paste_notes(track_id, &alone, Q * 6).unwrap();
        let track = project.track(track_id).unwrap();
        let note = track
            .notes
            .iter()
            .find(|n| n.id == outcome.note_ids[0])
            .unwrap();
        assert_eq!(note.tied_from_previous, None);
    }

    #[test]
    fn copied_notes_become_midi_notes_with_ties_merged() {
        let (mut project, track_id) = project_with(&[(0, Q, 0, 3), (Q, Q, 0, 3), (Q * 2, Q, 1, 2)]);
        project.track_mut(track_id).unwrap().notes[1].tied_from_previous = Some(TieOrigin::User);
        let clip = clip_of_all(&project, track_id);
        let track = project.track(track_id).unwrap();
        let notes = clip.midi_notes();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].key, track.tuning[0] + 3);
        assert_eq!(notes[0].duration, Q * 2);
        assert_eq!(notes[1].start, Q * 2);
        assert_eq!(notes[1].key, track.tuning[1] + 2);
    }

    #[test]
    fn the_text_tab_puts_the_highest_string_on_top() {
        let (project, track_id) = project_with(&[(0, Q, 0, 3), (Q, Q, 1, 12)]);
        let text = clip_of_all(&project, track_id).ascii_tab();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), project.track(track_id).unwrap().tuning.len());
        assert_eq!(*lines.last().unwrap(), "|-3----|");
        assert!(lines[lines.len() - 2].ends_with("12-|"));
        assert!(lines.iter().all(|line| line.len() == lines[0].len()));
    }

    #[test]
    fn midi_notes_land_on_the_lowest_fret_of_a_free_string() {
        let (project, track_id) = project_with(&[]);
        let track = project.track(track_id).unwrap();
        let open = track.tuning[1];
        let notes = [
            SmfNote { start: 960, duration: Q, key: open + 2, velocity: 80 },
            SmfNote { start: 960, duration: Q, key: open + 2, velocity: 80 },
            SmfNote { start: 960, duration: Q, key: 1, velocity: 80 },
        ];
        let clip = NoteClip::from_midi(&notes, track).unwrap();
        // The lone out-of-range key is left out; the doubled key moves to
        // another string that can play it.
        assert_eq!(clip.len(), 2);
        assert_eq!(clip.notes[0].offset, 0);
        let strings: Vec<usize> = clip.notes.iter().map(|note| note.string_index).collect();
        assert_ne!(strings[0], strings[1]);
        for note in &clip.notes {
            assert_eq!(track.tuning[note.string_index] + note.fret, open + 2);
        }
        assert_eq!(NoteClip::from_midi(&notes[2..], track), None);
    }

    #[test]
    fn a_midi_clip_pastes_like_a_copy() {
        let (mut project, track_id) = project_with(&[]);
        let open = project.track(track_id).unwrap().tuning[0];
        let notes = [SmfNote { start: 0, duration: Q, key: open + 5, velocity: 100 }];
        let clip = NoteClip::from_midi(&notes, project.track(track_id).unwrap()).unwrap();
        let outcome = project.paste_notes(track_id, &clip, Q).unwrap();
        assert_eq!(outcome.note_ids.len(), 1);
        assert_eq!(onsets(&project, track_id), [(Q, 0, 5)]);
    }
}
