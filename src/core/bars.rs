//! Bar-level structure of a track: bar spans, repeat signs and how they
//! unroll into playback order, and moving whole bars along or between tracks.

use std::collections::HashSet;

use super::project::{
    BarMark, BwxProject, InstrumentTrack, TabNote, TimeSignature, TimeSignatureChange, Tick,
};

/// Most extra plays a repeat close can ask for: 999 plays in all, far past
/// any real score and still cheap to unroll.
pub const MAX_REPEAT_COUNT: u16 = 998;

/// For each bar that closes a repeat, the bar its region starts at: the
/// matching open sign, else the bar after the previous close, else the first.
fn repeat_region_starts(marks: &[BarMark]) -> Vec<Option<usize>> {
    let mut region_start: Vec<Option<usize>> = vec![None; marks.len()];
    let mut open_stack: Vec<usize> = Vec::new();
    let mut previous_close_end = 0;
    for (index, mark) in marks.iter().enumerate() {
        if mark.repeat_open {
            open_stack.push(index);
        }
        if mark.repeat_close > 0 {
            region_start[index] = Some(open_stack.pop().unwrap_or(previous_close_end));
            previous_close_end = index + 1;
        }
    }
    region_start
}

/// Guards the unrolled bar list against pathological nesting.
const MAX_PLAYED_BARS: usize = 100_000;

/// A bar of a track, named by its 0-based index in the score.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BarRef {
    pub track_id: u64,
    pub bar: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarSpan {
    pub start: Tick,
    pub end: Tick,
    pub signature: TimeSignature,
}

/// One bar as it is played. `start` and `end` are on the unrolled playback
/// axis, where a repeated region occupies time once per play; `repeated` is
/// set from the second play of a bar on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayedBar {
    pub bar: usize,
    pub start: Tick,
    pub end: Tick,
    pub repeated: bool,
}

impl InstrumentTrack {
    pub fn bar_spans(&self) -> Vec<BarSpan> {
        let mut spans = Vec::with_capacity(self.bar_count.max(1) as usize);
        let mut cursor = 0;
        for _ in 0..self.bar_count.max(1) {
            let (start, end) = self.measure_bounds_at(cursor);
            spans.push(BarSpan {
                start,
                end,
                signature: self.signature_at(start),
            });
            cursor = end;
        }
        spans
    }

    pub fn bar_mark_at(&self, bar_start: Tick) -> BarMark {
        self.bar_marks
            .iter()
            .find(|mark| mark.at_tick == bar_start)
            .copied()
            .unwrap_or(BarMark {
                at_tick: bar_start,
                ..BarMark::default()
            })
    }

    fn edit_bar_mark(&mut self, bar_start: Tick, edit: impl FnOnce(&mut BarMark)) {
        let mut mark = self.bar_mark_at(bar_start);
        edit(&mut mark);
        self.bar_marks.retain(|existing| existing.at_tick != bar_start);
        if !mark.is_empty() {
            self.bar_marks.push(mark);
            self.bar_marks.sort_by_key(|mark| mark.at_tick);
        }
    }

    pub fn set_repeat_open(&mut self, bar_start: Tick, open: bool) {
        self.edit_bar_mark(bar_start, |mark| mark.repeat_open = open);
    }

    /// Sets how many extra times the region ending at this bar is played;
    /// 0 removes the close sign.
    pub fn set_repeat_close(&mut self, bar_start: Tick, repeats: u16) {
        self.edit_bar_mark(bar_start, |mark| {
            mark.repeat_close = repeats.min(MAX_REPEAT_COUNT);
        });
    }

    /// Moves every mark at or after `from` by `delta` ticks, the way bar
    /// edits move the bars the marks belong to.
    pub fn shift_bar_marks(&mut self, from: Tick, delta: Tick) {
        for mark in &mut self.bar_marks {
            if mark.at_tick >= from {
                mark.at_tick += delta;
            }
        }
    }

    pub fn has_repeats(&self) -> bool {
        self.bar_marks.iter().any(|mark| mark.repeat_close > 0)
    }

    /// The bars of the smallest repeated region of more than one bar that
    /// contains `bar`, or just `bar` when it is in none.
    pub fn loop_bars(&self, bar: usize) -> Vec<usize> {
        let marks: Vec<BarMark> = self
            .bar_spans()
            .iter()
            .map(|span| self.bar_mark_at(span.start))
            .collect();
        repeat_region_starts(&marks)
            .into_iter()
            .enumerate()
            .filter_map(|(close, start)| Some((start?, close)))
            .filter(|&(start, close)| start < close && (start..=close).contains(&bar))
            .min_by_key(|&(start, close)| close - start)
            .map_or_else(|| vec![bar], |(start, close)| (start..=close).collect())
    }

    /// Bars in the order they are played, with repeat signs unrolled. A
    /// close sign jumps back to the matching open sign (or to the bar after
    /// the previous close, or the start), once per extra play.
    pub fn played_bars(&self) -> Vec<PlayedBar> {
        let spans = self.bar_spans();
        let marks: Vec<BarMark> = spans
            .iter()
            .map(|span| self.bar_mark_at(span.start))
            .collect();
        let region_start = repeat_region_starts(&marks);

        let mut order: Vec<usize> = Vec::new();
        let mut plays_done = vec![0u16; spans.len()];
        let mut index = 0;
        while index < spans.len() && order.len() < MAX_PLAYED_BARS {
            order.push(index);
            let extra = marks[index].repeat_close.min(MAX_REPEAT_COUNT);
            if let Some(start) = region_start[index]
                && plays_done[index] < extra
            {
                plays_done[index] += 1;
                // Inner regions replay in full on every pass of this one.
                for done in &mut plays_done[start..index] {
                    *done = 0;
                }
                index = start;
                continue;
            }
            index += 1;
        }

        let mut seen = vec![false; spans.len()];
        let mut cursor = 0;
        order
            .into_iter()
            .map(|bar| {
                let length = spans[bar].end - spans[bar].start;
                let played = PlayedBar {
                    bar,
                    start: cursor,
                    end: cursor + length,
                    repeated: seen[bar],
                };
                seen[bar] = true;
                cursor += length;
                played
            })
            .collect()
    }

    /// Where the song position `tick` of the first play-through sits on the
    /// unrolled playback axis.
    pub fn playback_tick_for(&self, tick: Tick) -> Tick {
        if !self.has_repeats() {
            return tick;
        }
        let spans = self.bar_spans();
        let Some(bar) = spans.iter().position(|span| tick < span.end) else {
            return tick;
        };
        self.played_bars()
            .iter()
            .find(|played| played.bar == bar)
            .map_or(tick, |played| played.start + (tick - spans[bar].start).max(0))
    }

    /// The score position a playback position stands on. A repeated region
    /// maps to the same score ticks on every pass.
    pub fn score_tick_at_playback(&self, position: f64) -> f64 {
        if !self.has_repeats() {
            return position;
        }
        let spans = self.bar_spans();
        self.played_bars()
            .iter()
            .find(|played| position >= played.start as f64 && position < played.end as f64)
            .map_or(position, |played| {
                spans[played.bar].start as f64 + (position - played.start as f64)
            })
    }

    /// A copy of the track whose notes follow playback order, or `None`
    /// when the track has no repeats. Notes that run on across a barline
    /// stay joined only where the next bar played is the next bar written.
    pub fn unrolled_for_playback(&self) -> Option<InstrumentTrack> {
        if !self.has_repeats() {
            return None;
        }
        let spans = self.bar_spans();
        let played = self.played_bars();
        let mut notes: Vec<TabNote> = Vec::new();
        for (position, bar) in played.iter().enumerate() {
            let span = spans[bar.bar];
            let follows_written = position
                .checked_sub(1)
                .is_some_and(|before| played[before].bar + 1 == bar.bar);
            let continues_written = played
                .get(position + 1)
                .is_some_and(|after| after.bar == bar.bar + 1);
            for note in self
                .notes
                .iter()
                .filter(|note| note.abs_tick >= span.start && note.abs_tick < span.end)
            {
                let mut copy = note.clone();
                copy.id = notes.len() as u64 + 1;
                copy.abs_tick = bar.start + (note.abs_tick - span.start);
                if note.end_tick() > span.end && !continues_written {
                    copy.duration_ticks = span.end - note.abs_tick;
                }
                if note.abs_tick == span.start
                    && note.tied_from_previous.is_some()
                    && !follows_written
                {
                    copy.tied_from_previous = None;
                }
                notes.push(copy);
            }
        }
        let mut track = self.clone();
        track.notes = notes;
        track.sort_notes();
        Some(track)
    }

    /// End of the unrolled song for this track: the end of the last played
    /// bar that holds a note.
    pub fn playback_end_tick(&self) -> Option<Tick> {
        let last_note_end = self.notes.iter().map(TabNote::end_tick).max()?;
        if !self.has_repeats() {
            return Some(self.measure_bounds_at(last_note_end - 1).1);
        }
        let spans = self.bar_spans();
        self.played_bars()
            .iter()
            .filter(|played| {
                let span = spans[played.bar];
                self.notes
                    .iter()
                    .any(|note| note.abs_tick < span.end && note.end_tick() > span.start)
            })
            .map(|played| played.end)
            .max()
    }
}

/// One bar lifted out of a track. Note ticks are relative to the bar start.
#[derive(Clone)]
struct BarContent {
    signature: TimeSignature,
    length: Tick,
    notes: Vec<TabNote>,
    mark: BarMark,
    origin: BarRef,
    old_previous: Option<BarRef>,
    tied_from_previous_bar: bool,
}

/// A bar to move: where it is now, the track it goes to and the position
/// among that track's remaining bars it is inserted before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarMove {
    pub from: BarRef,
    pub to_track: u64,
    pub insert_before: usize,
}

/// What a bar move cost: the moved bars that lost a tie to a neighbour, and
/// notes pulled onto a lower string because the destination has fewer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BarMoveReport {
    pub broken_ties: Vec<BarRef>,
    pub clamped_notes: usize,
    /// Each moved bar's reference before the move and where it is after.
    pub landed: Vec<(BarRef, BarRef)>,
}

fn lift_bars(track: &InstrumentTrack) -> Vec<BarContent> {
    let spans = track.bar_spans();
    let last = spans.len() - 1;
    let mut bars: Vec<BarContent> = spans
        .iter()
        .enumerate()
        .map(|(index, span)| BarContent {
            signature: span.signature,
            length: span.end - span.start,
            notes: Vec::new(),
            mark: track.bar_mark_at(span.start),
            origin: BarRef {
                track_id: track.id,
                bar: index,
            },
            old_previous: index.checked_sub(1).map(|bar| BarRef {
                track_id: track.id,
                bar,
            }),
            tied_from_previous_bar: false,
        })
        .collect();
    for note in &track.notes {
        let index = spans
            .iter()
            .position(|span| note.abs_tick < span.end)
            .unwrap_or(last);
        let start = spans[index].start;
        let mut relative = note.clone();
        relative.abs_tick -= start;
        if relative.abs_tick == 0 && track.is_tied_continuation(note) {
            bars[index].tied_from_previous_bar = true;
        }
        bars[index].notes.push(relative);
    }
    bars
}

fn rebuild_track(track: &mut InstrumentTrack, bars: Vec<BarContent>) {
    let mut start = 0;
    let mut notes = Vec::new();
    let mut changes: Vec<TimeSignatureChange> = Vec::new();
    let mut marks = Vec::new();
    let mut previous: Option<(TimeSignature, bool)> = None;
    for bar in &bars {
        let needs_change = match previous {
            None => true,
            Some((signature, partial)) => signature != bar.signature || partial,
        };
        if needs_change {
            changes.push(TimeSignatureChange {
                at_tick: start,
                signature: bar.signature,
            });
        }
        previous = Some((bar.signature, bar.length != bar.signature.measure_ticks()));
        for note in &bar.notes {
            let mut placed = note.clone();
            placed.abs_tick += start;
            notes.push(placed);
        }
        if !bar.mark.is_empty() {
            marks.push(BarMark {
                at_tick: start,
                ..bar.mark
            });
        }
        start += bar.length;
    }
    track.bar_count = bars.len() as u32;
    track.time_signature_changes = changes;
    track.notes = notes;
    track.bar_marks = marks;
    track.sort_notes();
}

impl BwxProject {
    /// Moves whole bars, with their notes, time signatures and repeat signs,
    /// along their track or onto other tracks. A bar that was tied to a
    /// neighbour it no longer sits next to loses that tie. Every note is cut
    /// at barlines first, so a brick running across a barline is cut there
    /// too. Returns `None` if a move names a bar or track that is not there.
    pub fn move_bars(&mut self, moves: &[BarMove]) -> Option<BarMoveReport> {
        if moves.is_empty() {
            return None;
        }
        let mut seen = HashSet::new();
        for bar_move in moves {
            let source = self.track(bar_move.from.track_id)?;
            self.track(bar_move.to_track)?;
            if bar_move.from.bar >= source.bar_spans().len()
                || !seen.insert(bar_move.from)
            {
                return None;
            }
        }
        self.split_bricks_at_barlines();

        let mut involved: Vec<u64> = Vec::new();
        for bar_move in moves {
            for id in [bar_move.from.track_id, bar_move.to_track] {
                if !involved.contains(&id) {
                    involved.push(id);
                }
            }
        }

        let mut report = BarMoveReport::default();
        let mut lifted: Vec<(u64, Vec<BarContent>)> = Vec::new();
        for &id in &involved {
            lifted.push((id, lift_bars(self.track(id)?)));
        }

        // Detach the moved bars from their source tracks.
        let mut carried: Vec<(BarMove, BarContent)> = Vec::new();
        for bar_move in moves {
            let bars = &mut lifted
                .iter_mut()
                .find(|(id, _)| *id == bar_move.from.track_id)?
                .1;
            let content = bars
                .iter()
                .find(|bar| bar.origin == bar_move.from)?
                .clone();
            carried.push((*bar_move, content));
        }
        for (id, bars) in &mut lifted {
            bars.retain(|bar| !moves.iter().any(|m| m.from == bar.origin && m.from.track_id == *id));
        }

        let moved_origins: HashSet<BarRef> = moves.iter().map(|m| m.from).collect();
        for (track_id, remaining) in lifted {
            let mut arranged: Vec<BarContent> = Vec::new();
            let incoming: Vec<&(BarMove, BarContent)> = carried
                .iter()
                .filter(|(bar_move, _)| bar_move.to_track == track_id)
                .collect();
            for position in 0..=remaining.len() {
                for (bar_move, content) in &incoming {
                    if bar_move.insert_before.min(remaining.len()) == position {
                        arranged.push(content.clone());
                    }
                }
                if let Some(bar) = remaining.get(position) {
                    arranged.push(bar.clone());
                }
            }
            if arranged.is_empty() {
                let signature = carried
                    .first()
                    .map_or(TimeSignature::new(4, 4), |(_, content)| content.signature);
                arranged.push(BarContent {
                    signature,
                    length: signature.measure_ticks(),
                    notes: Vec::new(),
                    mark: BarMark::default(),
                    origin: BarRef { track_id, bar: usize::MAX },
                    old_previous: None,
                    tied_from_previous_bar: false,
                });
            }

            // A tie survives only while the bar it came from is still the
            // bar written before.
            for index in 0..arranged.len() {
                let kept = arranged[index].old_previous.is_some()
                    && index
                        .checked_sub(1)
                        .is_some_and(|before| Some(arranged[before].origin) == arranged[index].old_previous);
                if arranged[index].tied_from_previous_bar && !kept {
                    for note in &mut arranged[index].notes {
                        if note.abs_tick == 0 {
                            note.tied_from_previous = None;
                        }
                    }
                    let content = &arranged[index];
                    for origin in [Some(content.origin), content.old_previous].into_iter().flatten() {
                        if moved_origins.contains(&origin) && !report.broken_ties.contains(&origin) {
                            report.broken_ties.push(origin);
                        }
                    }
                }
            }

            let string_count = self.track(track_id)?.tuning.len();
            for bar in &mut arranged {
                for note in &mut bar.notes {
                    if note.string_index >= string_count {
                        note.string_index = string_count.saturating_sub(1);
                        report.clamped_notes += 1;
                    }
                }
            }
            for (index, bar) in arranged.iter().enumerate() {
                if moved_origins.contains(&bar.origin) {
                    report.landed.push((
                        bar.origin,
                        BarRef {
                            track_id,
                            bar: index,
                        },
                    ));
                }
            }
            rebuild_track(self.track_mut(track_id)?, arranged);
        }
        report.broken_ties.sort();
        report.landed.sort();
        Some(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TICKS_PER_QUARTER;

    const BAR: Tick = TICKS_PER_QUARTER * 4;

    fn project_with_bars(count: u32) -> BwxProject {
        let mut project = BwxProject::demo();
        project.tracks.truncate(1);
        let track = &mut project.tracks[0];
        track.notes.clear();
        track.bar_marks.clear();
        track.bar_count = count;
        project
    }

    fn note_at_bar(project: &mut BwxProject, track_id: u64, bar: Tick, fret: u8) -> u64 {
        project
            .insert_note(track_id, bar * BAR, TICKS_PER_QUARTER, 0, fret)
            .expect("note fits")
    }

    fn bar_of_fret(track: &InstrumentTrack, fret: u8) -> Tick {
        track
            .notes
            .iter()
            .find(|note| note.fret == fret)
            .map(|note| note.abs_tick / BAR)
            .expect("note present")
    }

    #[test]
    fn a_track_without_repeat_signs_plays_each_bar_once() {
        let project = project_with_bars(3);
        let played = project.tracks[0].played_bars();
        assert_eq!(played.iter().map(|p| p.bar).collect::<Vec<_>>(), [0, 1, 2]);
        assert!(played.iter().all(|p| !p.repeated));
    }

    #[test]
    fn a_repeat_close_replays_back_to_the_open_sign() {
        let mut project = project_with_bars(4);
        let track = &mut project.tracks[0];
        track.set_repeat_open(BAR, true);
        track.set_repeat_close(BAR * 2, 1);
        let order: Vec<usize> = track.played_bars().iter().map(|p| p.bar).collect();
        assert_eq!(order, [0, 1, 2, 1, 2, 3]);
        let repeated: Vec<bool> = track.played_bars().iter().map(|p| p.repeated).collect();
        assert_eq!(repeated, [false, false, false, true, true, false]);
        assert_eq!(track.played_bars().last().unwrap().end, BAR * 6);
    }

    #[test]
    fn a_bar_of_a_loop_names_every_bar_of_the_smallest_multi_bar_loop() {
        let mut project = project_with_bars(5);
        let track = &mut project.tracks[0];
        track.set_repeat_open(BAR, true);
        track.set_repeat_close(BAR * 3, 1);
        assert_eq!(track.loop_bars(2), [1, 2, 3]);
        assert_eq!(track.loop_bars(1), [1, 2, 3]);
        assert_eq!(track.loop_bars(0), [0]);
        assert_eq!(track.loop_bars(4), [4]);
        track.set_repeat_open(BAR * 2, true);
        track.set_repeat_close(BAR * 2, 1);
        // A one-bar loop inside a larger one: the larger one is the group.
        assert_eq!(track.loop_bars(2), [1, 2, 3]);
    }

    #[test]
    fn a_close_without_an_open_repeats_from_the_start() {
        let mut project = project_with_bars(2);
        let track = &mut project.tracks[0];
        track.set_repeat_close(BAR, 2);
        let order: Vec<usize> = track.played_bars().iter().map(|p| p.bar).collect();
        assert_eq!(order, [0, 1, 0, 1, 0, 1]);
    }

    #[test]
    fn nested_repeats_replay_the_inner_one_on_every_pass() {
        let mut project = project_with_bars(3);
        let track = &mut project.tracks[0];
        track.set_repeat_open(0, true);
        track.set_repeat_open(BAR, true);
        track.set_repeat_close(BAR, 1);
        track.set_repeat_close(BAR * 2, 1);
        let order: Vec<usize> = track.played_bars().iter().map(|p| p.bar).collect();
        assert_eq!(order, [0, 1, 1, 2, 0, 1, 1, 2]);
    }

    #[test]
    fn playback_positions_map_back_to_the_score() {
        let mut project = project_with_bars(3);
        let track = &mut project.tracks[0];
        track.set_repeat_close(BAR, 1);
        let on_second_pass = (BAR * 2 + TICKS_PER_QUARTER) as f64;
        assert_eq!(
            track.score_tick_at_playback(on_second_pass),
            TICKS_PER_QUARTER as f64
        );
        assert_eq!(track.playback_tick_for(BAR + 10), BAR + 10);
        assert_eq!(track.playback_tick_for(BAR * 2), BAR * 4);
    }

    #[test]
    fn unrolled_playback_copies_notes_once_per_play() {
        let mut project = project_with_bars(2);
        note_at_bar(&mut project, 1, 0, 3);
        project.tracks[0].set_repeat_close(0, 1);
        let unrolled = project.tracks[0].unrolled_for_playback().unwrap();
        let ticks: Vec<Tick> = unrolled.notes.iter().map(|note| note.abs_tick).collect();
        assert_eq!(ticks, [0, BAR]);
        assert_eq!(project.tracks[0].playback_end_tick(), Some(BAR * 2));
    }

    #[test]
    fn insert_and_delete_bar_keep_marks_on_their_bars() {
        let mut project = project_with_bars(3);
        project.tracks[0].set_repeat_open(BAR, true);
        assert!(project.insert_bar_before(1, 0));
        assert_eq!(project.tracks[0].bar_marks[0].at_tick, BAR * 2);
        assert!(project.delete_bar(1, 0));
        assert_eq!(project.tracks[0].bar_marks[0].at_tick, BAR);
        assert!(project.add_bar_after(1, BAR));
        assert_eq!(project.tracks[0].bar_marks[0].at_tick, BAR);
        assert!(project.delete_bar(1, BAR));
        assert!(project.tracks[0].bar_marks.is_empty());
    }

    #[test]
    fn moving_a_bar_along_its_track_reorders_the_bars() {
        let mut project = project_with_bars(3);
        for bar in 0..3 {
            note_at_bar(&mut project, 1, bar, bar as u8 + 1);
        }
        let report = project
            .move_bars(&[BarMove {
                from: BarRef { track_id: 1, bar: 0 },
                to_track: 1,
                insert_before: 2,
            }])
            .unwrap();
        assert!(report.broken_ties.is_empty());
        let track = &project.tracks[0];
        assert_eq!(bar_of_fret(track, 2), 0);
        assert_eq!(bar_of_fret(track, 3), 1);
        assert_eq!(bar_of_fret(track, 1), 2);
        assert_eq!(track.bar_count, 3);
    }

    #[test]
    fn moving_a_bar_to_another_track_removes_it_from_the_source() {
        let mut project = project_with_bars(2);
        let id = project.add_track_like(1).unwrap();
        note_at_bar(&mut project, 1, 1, 7);
        project
            .move_bars(&[BarMove {
                from: BarRef { track_id: 1, bar: 1 },
                to_track: id,
                insert_before: 0,
            }])
            .unwrap();
        assert_eq!(project.tracks[0].bar_count, 1);
        assert!(project.tracks[0].notes.is_empty());
        assert_eq!(project.tracks[1].bar_count, 3);
        assert_eq!(bar_of_fret(&project.tracks[1], 7), 0);
    }

    #[test]
    fn a_bar_keeps_its_time_signature_and_repeat_signs_when_moved() {
        let mut project = project_with_bars(3);
        project.set_track_time_signature(1, BAR * 2, TimeSignature::new(3, 4));
        project.tracks[0].set_repeat_close(BAR * 2, 1);
        project
            .move_bars(&[BarMove {
                from: BarRef { track_id: 1, bar: 2 },
                to_track: 1,
                insert_before: 0,
            }])
            .unwrap();
        let track = &project.tracks[0];
        let spans = track.bar_spans();
        assert_eq!(spans[0].signature, TimeSignature::new(3, 4));
        assert_eq!(spans[1].signature, TimeSignature::new(4, 4));
        assert_eq!(track.bar_mark_at(0).repeat_close, 1);
    }

    #[test]
    fn moving_a_bar_away_from_a_tied_neighbour_breaks_the_tie_and_reports_it() {
        let mut project = project_with_bars(3);
        project
            .insert_note(1, BAR - TICKS_PER_QUARTER, TICKS_PER_QUARTER * 2, 0, 5)
            .unwrap();
        let report = project
            .move_bars(&[BarMove {
                from: BarRef { track_id: 1, bar: 1 },
                to_track: 1,
                insert_before: 2,
            }])
            .unwrap();
        assert_eq!(report.broken_ties, [BarRef { track_id: 1, bar: 1 }]);
        let track = &project.tracks[0];
        assert!(track.notes.iter().all(|note| !track.is_tied_continuation(note)));
        assert!(track.notes.iter().all(|note| note.tied_from_previous.is_none()
            || note.abs_tick != BAR * 2));
    }

    #[test]
    fn moving_both_halves_of_a_tie_together_keeps_it() {
        let mut project = project_with_bars(3);
        project
            .insert_note(1, BAR - TICKS_PER_QUARTER, TICKS_PER_QUARTER * 2, 0, 5)
            .unwrap();
        let report = project
            .move_bars(&[
                BarMove {
                    from: BarRef { track_id: 1, bar: 0 },
                    to_track: 1,
                    insert_before: 2,
                },
                BarMove {
                    from: BarRef { track_id: 1, bar: 1 },
                    to_track: 1,
                    insert_before: 2,
                },
            ])
            .unwrap();
        assert!(report.broken_ties.is_empty());
        let track = &project.tracks[0];
        assert!(track.notes.iter().any(|note| track.is_tied_continuation(note)));
    }

    #[test]
    fn moving_every_bar_out_leaves_one_empty_bar() {
        let mut project = project_with_bars(1);
        let id = project.add_track_like(1).unwrap();
        note_at_bar(&mut project, 1, 0, 4);
        project
            .move_bars(&[BarMove {
                from: BarRef { track_id: 1, bar: 0 },
                to_track: id,
                insert_before: 1,
            }])
            .unwrap();
        assert_eq!(project.tracks[0].bar_count, 1);
        assert!(project.tracks[0].notes.is_empty());
        assert_eq!(project.tracks[1].bar_count, 2);
    }

    #[test]
    fn a_move_naming_a_missing_bar_changes_nothing() {
        let mut project = project_with_bars(2);
        let before = project.clone();
        assert!(
            project
                .move_bars(&[BarMove {
                    from: BarRef { track_id: 1, bar: 9 },
                    to_track: 1,
                    insert_before: 0,
                }])
                .is_none()
        );
        assert_eq!(project, before);
    }
}
