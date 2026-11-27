//! Bar-level structure of a track: bar spans, repeat signs and how they
//! unroll into playback order, and moving whole bars along or between tracks.

use std::collections::{HashMap, HashSet};

use super::annotations::TextAnnotation;
use super::pitch::TremoloDive;
use super::tuplets::TupletSpan;
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
        self.unrolled_with_origins(None).map(|unrolled| unrolled.track)
    }

    /// `unrolled_for_playback` with only the bars played whose entry in
    /// `keep` is set (every bar when `None`), placed one after another.
    fn unrolled_with_origins(&self, keep: Option<&[bool]>) -> Option<Unrolled> {
        if !self.has_repeats() {
            return None;
        }
        let spans = self.bar_spans();
        let mut cursor = 0;
        let bars: Vec<(PlayedBar, BarSpan)> = self
            .played_bars()
            .into_iter()
            .enumerate()
            .filter(|(index, _)| keep.is_none_or(|keep| keep.get(*index).copied().unwrap_or(true)))
            .map(|(_, played)| {
                let span = spans[played.bar];
                let length = span.end - span.start;
                let placed = PlayedBar {
                    bar: played.bar,
                    start: cursor,
                    end: cursor + length,
                    repeated: played.repeated,
                };
                cursor += length;
                (placed, span)
            })
            .collect();
        let mut notes: Vec<TabNote> = Vec::new();
        let mut origins: HashMap<u64, u64> = HashMap::new();
        for (position, (bar, span)) in bars.iter().enumerate() {
            let follows_written = position
                .checked_sub(1)
                .is_some_and(|before| bars[before].0.bar + 1 == bar.bar);
            let continues_written = bars
                .get(position + 1)
                .is_some_and(|(after, _)| after.bar == bar.bar + 1);
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
                origins.insert(copy.id, note.id);
                notes.push(copy);
            }
        }
        let mut track = self.clone();
        track.notes = notes;
        track.tremolo_bar = bars
            .iter()
            .flat_map(|(bar, span)| {
                self.tremolo_bar
                    .iter()
                    .filter(|dive| dive.start >= span.start && dive.start < span.end)
                    .map(|dive| TremoloDive { start: bar.start + (dive.start - span.start), ..dive.clone() })
            })
            .collect();
        track.sort_notes();
        Some(Unrolled { track, origins, bars })
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

struct Unrolled {
    track: InstrumentTrack,
    origins: HashMap<u64, u64>,
    bars: Vec<(PlayedBar, BarSpan)>,
}

/// A track with its repeats written out in playing order, for editing a
/// repeated section where its passes are on show. Each copy of a note keeps
/// the id of the written note it came from in `origin`, so an edit made on
/// any pass can be made once, on the written section, and shows on all.
#[derive(Clone, Debug)]
pub struct UnrolledView {
    /// The unrolled track: one bar per bar shown, no repeat signs.
    pub track: InstrumentTrack,
    origins: HashMap<u64, u64>,
    /// Each bar shown, with the written bar it repeats.
    bars: Vec<(PlayedBar, BarSpan)>,
    /// Each bar played, as `(start, end, shown start)`: where it stands in
    /// playing order and where on the unrolled track it is drawn.
    playback: Vec<(Tick, Tick, Tick)>,
}

impl UnrolledView {
    /// Every pass of every repeat shown; `None` when the track has no
    /// repeats and is shown as written.
    #[cfg(test)]
    pub fn of(track: &InstrumentTrack) -> Option<Self> {
        Self::partial(track, None)
    }

    /// Only the passes whose entry in `keep` (one per bar played) is set are
    /// shown; a pass left out is drawn as the first pass of its bar, which
    /// must be kept.
    pub fn partial(track: &InstrumentTrack, keep: Option<&[bool]>) -> Option<Self> {
        let Unrolled { track: mut view, origins, bars } = track.unrolled_with_origins(keep)?;
        let mut shown_so_far = 0;
        let playback: Vec<(Tick, Tick, Tick)> = track
            .played_bars()
            .iter()
            .enumerate()
            .map(|(index, played)| {
                let kept = keep.is_none_or(|keep| keep.get(index).copied().unwrap_or(true));
                let shown = if kept {
                    shown_so_far += 1;
                    bars.get(shown_so_far - 1).map(|(bar, _)| bar.start)
                } else {
                    bars.iter()
                        .find(|(bar, _)| bar.bar == played.bar)
                        .map(|(bar, _)| bar.start)
                };
                (played.start, played.end, shown.unwrap_or(played.start))
            })
            .collect();

        let mut changes: Vec<TimeSignatureChange> = Vec::new();
        let mut previous: Option<(TimeSignature, bool)> = None;
        for (played, span) in &bars {
            let needs_change = match previous {
                None => true,
                Some((signature, partial)) => signature != span.signature || partial,
            };
            if needs_change {
                changes.push(TimeSignatureChange {
                    at_tick: played.start,
                    signature: span.signature,
                });
            }
            previous = Some((
                span.signature,
                span.end - span.start != span.signature.measure_ticks(),
            ));
        }
        view.time_signature_changes = changes;
        view.bar_marks.clear();
        view.bar_count = bars.len() as u32;
        Some(Self { track: view, origins, bars, playback })
    }

    /// Where a playing position (on the track's unrolled playing axis) is
    /// drawn. A pass that is not shown is drawn on its first pass.
    pub fn shown_tick_at_playback(&self, position: f64) -> f64 {
        match self
            .playback
            .iter()
            .find(|(start, end, _)| position >= *start as f64 && position < *end as f64)
        {
            Some((start, _, shown)) => *shown as f64 + (position - *start as f64),
            None => match (self.playback.last(), self.bars.last()) {
                (Some((_, end, _)), Some((last, _))) => last.end as f64 + (position - *end as f64),
                _ => position,
            },
        }
    }

    /// The written note a note of the unrolled track is a copy of. An id
    /// that is not a copy is returned unchanged.
    pub fn origin(&self, copy_id: u64) -> u64 {
        self.origins.get(&copy_id).copied().unwrap_or(copy_id)
    }

    /// The written tick under `tick` of the unrolled track. Past the last
    /// bar it carries on from the end of the last written bar played.
    pub fn written_tick(&self, tick: Tick) -> Tick {
        let found = self
            .bars
            .iter()
            .find(|(played, _)| tick >= played.start && tick < played.end);
        match (found, self.bars.last()) {
            (Some((played, span)), _) => span.start + (tick - played.start),
            (None, Some((played, span))) if tick >= played.end => span.end + (tick - played.end),
            _ => tick,
        }
    }

    /// Whether `tick` of the unrolled track lies in a bar that is a repeat
    /// of one already played.
    pub fn is_repeated_at(&self, tick: Tick) -> bool {
        self.bars
            .iter()
            .any(|(played, _)| played.repeated && tick >= played.start && tick < played.end)
    }

    /// Every place a written tick is shown: once per pass of its bar. A
    /// tick on the end of the last written bar sits on the end of its last
    /// pass.
    pub fn view_ticks(&self, tick: Tick) -> Vec<Tick> {
        let mut ticks: Vec<Tick> = self
            .bars
            .iter()
            .filter(|(_, span)| tick >= span.start && tick < span.end)
            .map(|(played, span)| played.start + (tick - span.start))
            .collect();
        if ticks.is_empty()
            && let Some((played, span)) = self.bars.iter().rev().find(|(_, span)| tick == span.end)
        {
            ticks.push(played.end + (tick - span.end));
        }
        ticks
    }
}

/// One bar lifted out of a track. Note ticks are relative to the bar start.
#[derive(Clone)]
struct BarContent {
    signature: TimeSignature,
    length: Tick,
    notes: Vec<TabNote>,
    /// Tuplet spans and texts of the bar, relative to its start.
    tuplets: Vec<TupletSpan>,
    texts: Vec<TextAnnotation>,
    dives: Vec<TremoloDive>,
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
            tuplets: track
                .tuplets
                .iter()
                .filter(|tuplet| tuplet.start >= span.start && tuplet.start < span.end)
                .map(|tuplet| TupletSpan { start: tuplet.start - span.start, ..*tuplet })
                .collect(),
            texts: track
                .annotations
                .iter()
                .filter(|text| text.tick >= span.start && text.tick < span.end)
                .map(|text| TextAnnotation { tick: text.tick - span.start, ..text.clone() })
                .collect(),
            dives: track
                .tremolo_bar
                .iter()
                .filter(|dive| dive.start >= span.start && dive.start < span.end)
                .map(|dive| TremoloDive { start: dive.start - span.start, ..dive.clone() })
                .collect(),
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
    let mut tuplets = Vec::new();
    let mut texts = Vec::new();
    let mut dives: Vec<TremoloDive> = Vec::new();
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
        tuplets.extend(bar.tuplets.iter().map(|tuplet| TupletSpan { start: tuplet.start + start, ..*tuplet }));
        texts.extend(bar.texts.iter().map(|text| TextAnnotation { tick: text.tick + start, ..text.clone() }));
        for dive in &bar.dives {
            // A dive that would run into the one before it is dropped.
            if dives.last().is_none_or(|last| last.end() <= dive.start + start) {
                dives.push(TremoloDive { start: dive.start + start, ..dive.clone() });
            }
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
    track.tuplets = tuplets;
    track.annotations = texts;
    track.tremolo_bar = dives;
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
                    tuplets: Vec::new(),
                    texts: Vec::new(),
                    dives: Vec::new(),
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

    fn looped_project() -> BwxProject {
        // Bars 0..4 with bar 1 and 2 repeated once: 0 1 2 1 2 3.
        let mut project = project_with_bars(4);
        let id = project.tracks[0].id;
        for (bar, fret) in [(0, 10), (1, 11), (2, 12), (3, 13)] {
            note_at_bar(&mut project, id, bar, fret);
        }
        let track = &mut project.tracks[0];
        track.set_repeat_open(BAR, true);
        track.set_repeat_close(BAR * 2, 1);
        project
    }

    #[test]
    fn the_unrolled_view_lists_every_bar_played() {
        let project = looped_project();
        let view = UnrolledView::of(&project.tracks[0]).expect("has repeats");
        assert_eq!(view.track.bar_count, 6);
        assert!(view.track.bar_marks.is_empty());
        let frets: Vec<u8> = view.track.notes.iter().map(|note| note.fret).collect();
        assert_eq!(frets, [10, 11, 12, 11, 12, 13]);
        let starts: Vec<Tick> = view.track.notes.iter().map(|note| note.abs_tick).collect();
        assert_eq!(starts, [0, BAR, BAR * 2, BAR * 3, BAR * 4, BAR * 5]);
    }

    #[test]
    fn only_later_passes_count_as_repeated() {
        let project = looped_project();
        let view = UnrolledView::of(&project.tracks[0]).unwrap();
        let flags: Vec<bool> = (0..6).map(|bar| view.is_repeated_at(bar * BAR + 1)).collect();
        assert_eq!(flags, [false, false, false, true, true, false]);
    }

    #[test]
    fn a_track_without_repeats_has_no_unrolled_view() {
        assert!(UnrolledView::of(&project_with_bars(3).tracks[0]).is_none());
    }

    #[test]
    fn every_copy_of_a_note_names_the_written_note() {
        let project = looped_project();
        let written = &project.tracks[0];
        let view = UnrolledView::of(written).unwrap();
        for copy in &view.track.notes {
            let origin = written
                .notes
                .iter()
                .find(|note| note.id == view.origin(copy.id))
                .expect("origin is a written note");
            assert_eq!(origin.fret, copy.fret);
        }
        let copies = view
            .track
            .notes
            .iter()
            .filter(|note| note.fret == 11)
            .map(|note| view.origin(note.id))
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 2);
        assert_eq!(copies[0], copies[1]);
    }

    #[test]
    fn view_ticks_map_back_to_written_ticks_on_every_pass() {
        let project = looped_project();
        let view = UnrolledView::of(&project.tracks[0]).unwrap();
        let tick = BAR + TICKS_PER_QUARTER;
        assert_eq!(
            view.view_ticks(tick),
            [BAR + TICKS_PER_QUARTER, BAR * 3 + TICKS_PER_QUARTER]
        );
        for shown in view.view_ticks(tick) {
            assert_eq!(view.written_tick(shown), tick);
        }
        assert_eq!(view.view_ticks(0), [0]);
        assert_eq!(view.written_tick(BAR * 5 + 7), BAR * 3 + 7);
        // The end of the last written bar is the end of the last pass.
        assert_eq!(view.view_ticks(BAR * 4), [BAR * 6]);
    }

    #[test]
    fn the_unrolled_view_follows_the_signature_of_each_bar_played() {
        let mut project = looped_project();
        let track = &mut project.tracks[0];
        // Bars 1 and 2 are 3/4; the rest stay 4/4. Notes sit at the new grid.
        track.notes.clear();
        track.time_signature_changes = vec![
            TimeSignatureChange { at_tick: 0, signature: TimeSignature::new(4, 4) },
            TimeSignatureChange { at_tick: BAR, signature: TimeSignature::new(3, 4) },
            TimeSignatureChange {
                at_tick: BAR + TICKS_PER_QUARTER * 6,
                signature: TimeSignature::new(4, 4),
            },
        ];
        track.set_repeat_open(BAR, true);
        track.set_repeat_close(BAR + TICKS_PER_QUARTER * 3, 1);
        let spans: Vec<TimeSignature> = track.bar_spans().iter().map(|span| span.signature).collect();
        let view = UnrolledView::of(track).unwrap();
        let shown: Vec<TimeSignature> = view.track.bar_spans().iter().map(|span| span.signature).collect();
        let order: Vec<usize> = track.played_bars().iter().map(|played| played.bar).collect();
        let expected: Vec<TimeSignature> = order.iter().map(|&bar| spans[bar]).collect();
        assert_eq!(shown, expected);
    }

    #[test]
    fn editing_the_written_note_changes_every_copy() {
        let mut project = looped_project();
        let written_id = {
            let view = UnrolledView::of(&project.tracks[0]).unwrap();
            let copy = view.track.notes.iter().find(|note| note.fret == 11).unwrap();
            view.origin(copy.id)
        };
        project.tracks[0]
            .notes
            .iter_mut()
            .find(|note| note.id == written_id)
            .unwrap()
            .fret = 20;
        let view = UnrolledView::of(&project.tracks[0]).unwrap();
        assert_eq!(view.track.notes.iter().filter(|note| note.fret == 20).count(), 2);
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
    fn a_moved_bar_takes_its_texts_and_tuplet_spans_along() {
        let mut project = project_with_bars(3);
        let q = crate::core::project::TICKS_PER_QUARTER;
        let bar = project.tracks[0].bar_spans()[0].end - project.tracks[0].bar_spans()[0].start;
        let id = project.insert_note(1, 0, q, 0, 3).unwrap();
        project.split_notes_into_tuplets(1, &[id], 3);
        project.tracks[0].set_annotation(10, "first");
        project.tracks[0].set_annotation(bar + 10, "second");
        project
            .move_bars(&[BarMove {
                from: BarRef { track_id: 1, bar: 0 },
                to_track: 1,
                insert_before: 2,
            }])
            .unwrap();
        let track = &project.tracks[0];
        let texts: Vec<_> = track.annotations.iter().map(|note| (note.tick, note.text.as_str())).collect();
        assert_eq!(texts, [(10, "second"), (bar * 2 + 10, "first")]);
        assert_eq!(track.tuplets.len(), 1);
        assert_eq!(track.tuplets[0].start, bar * 2);
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
