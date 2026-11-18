//! The bar timeline beside the track table: one row of bar blocks per track
//! on a shared time axis in seconds, so a track at a different tempo gets
//! blocks of a different width. Blocks can be selected (alone, with Ctrl, or as a
//! Shift range), compared against the block under the pointer, and dragged
//! along or across tracks. A repeated region shows its extra plays as copies
//! that belong to the bars they copy.

use super::*;
use crate::core::tempo::{BarRun, Resync, Seconds, TempoMap, add_fractions, resync};

/// The bar timeline hides when the panel leaves less room than this.
const TIMELINE_MIN_WIDTH: f32 = 120.0;
/// The shortest bar of the project never gets narrower than this.
const TIMELINE_MIN_BLOCK: f32 = 30.0;
/// Pixels per second when only a few bars share the whole width: a 4/4 bar
/// at 120 bpm stops growing at about 170 points.
const TIMELINE_MAX_SCALE: f32 = 0.045 * (2 * TICKS_PER_QUARTER) as f32;
/// A block narrower than this shows only its bar number.
const TIMELINE_LABELLED_WIDTH: f32 = 54.0;
/// The pointer has to travel this far before a press on a block is a drag.
const TIMELINE_DRAG_THRESHOLD: f32 = 4.0;

pub(super) struct TimelineBar {
    /// Place on the playback axis, where a repeated region takes room once
    /// per play.
    pub start: Tick,
    pub end: Tick,
    /// The same span in seconds through the track's tempo curve.
    pub from: f64,
    pub to: f64,
    /// The span as exact fractions of a second; `None` after a tempo ramp.
    pub exact_from: Option<Seconds>,
    pub exact_length: Option<Seconds>,
    pub signature: TimeSignature,
    pub has_notes: bool,
    /// The bar's index in the score and the tick it starts at there.
    pub bar: usize,
    pub score_start: Tick,
    /// A later play of a bar already shown earlier in the row.
    pub repeated: bool,
}

pub(super) struct TimelineRow {
    pub track_id: u64,
    pub name: String,
    pub bars: Vec<TimelineBar>,
}

impl TimelineRow {
    pub fn new(track: &InstrumentTrack, tempo: &TempoMap) -> Self {
        let spans = track.bar_spans();
        let mut exact_from = Some((0, 1));
        let bars = track
            .played_bars()
            .into_iter()
            .map(|played| {
                let span = spans[played.bar];
                let exact_length = tempo.exact_seconds(played.start, played.end);
                let block_from = exact_from;
                exact_from = exact_from
                    .zip(exact_length)
                    .and_then(|(from, length)| add_fractions(from, length));
                TimelineBar {
                    start: played.start,
                    end: played.end,
                    from: tempo.seconds_at(played.start as f64),
                    to: tempo.seconds_at(played.end as f64),
                    exact_from: block_from,
                    exact_length,
                    signature: span.signature,
                    has_notes: track
                        .notes
                        .iter()
                        .any(|note| note.abs_tick < span.end && note.end_tick() > span.start),
                    bar: played.bar,
                    score_start: span.start,
                    repeated: played.repeated,
                }
            })
            .collect();
        Self {
            track_id: track.id,
            name: track.name.clone(),
            bars,
        }
    }

    /// The first play of `bar`.
    fn original(&self, bar: usize) -> Option<&TimelineBar> {
        self.bars
            .iter()
            .find(|block| block.bar == bar && !block.repeated)
    }

    fn block_at(&self, seconds: f64) -> Option<&TimelineBar> {
        self.bars
            .iter()
            .find(|block| seconds >= block.from && seconds < block.to)
    }
}

/// A press on a block that has moved far enough to be a drag.
#[derive(Clone, Copy)]
pub(super) struct TimelineDrag {
    press: Pos2,
    pointer: Pos2,
}

impl TimelineDrag {
    fn is_moving(&self) -> bool {
        (self.pointer - self.press).length() >= TIMELINE_DRAG_THRESHOLD
    }
}

/// Pixels per second for the bar timeline: everything fits `width` when it
/// can, but never so small that the shortest bar drops below the minimum
/// block width, and never so large that a few bars stretch absurdly.
pub(super) fn timeline_scale(rows: &[TimelineRow], width: f32) -> f32 {
    let total = rows
        .iter()
        .filter_map(|row| row.bars.last())
        .map(|bar| bar.to)
        .fold(0.0_f64, f64::max)
        .max(1e-6) as f32;
    let shortest = rows
        .iter()
        .flat_map(|row| row.bars.iter())
        .map(|bar| bar.to - bar.from)
        .fold(f64::INFINITY, f64::min)
        .max(1e-6) as f32;
    (width / total)
        .min(TIMELINE_MAX_SCALE)
        .max(TIMELINE_MIN_BLOCK / shortest)
}

/// Bars a Shift-click selects: a run of one track, or, across tracks, every
/// bar of the rows in between that overlaps the time between the two.
fn range_between(rows: &[TimelineRow], anchor: BarRef, target: BarRef) -> Vec<BarRef> {
    let position = |id: u64| rows.iter().position(|row| row.track_id == id);
    let (Some(anchor_row), Some(target_row)) = (position(anchor.track_id), position(target.track_id))
    else {
        return vec![target];
    };
    if anchor_row == target_row {
        let (low, high) = (anchor.bar.min(target.bar), anchor.bar.max(target.bar));
        return (low..=high)
            .map(|bar| BarRef {
                track_id: target.track_id,
                bar,
            })
            .collect();
    }
    let (Some(from), Some(to)) = (
        rows[anchor_row].original(anchor.bar),
        rows[target_row].original(target.bar),
    ) else {
        return vec![target];
    };
    let (start, end) = (from.from.min(to.from), from.to.max(to.to));
    rows[anchor_row.min(target_row)..=anchor_row.max(target_row)]
        .iter()
        .flat_map(|row| {
            row.bars
                .iter()
                .filter(|block| !block.repeated && block.from < end && block.to > start)
                .map(|block| BarRef {
                    track_id: row.track_id,
                    bar: block.bar,
                })
        })
        .collect()
}

/// The moves a drag of `delta_seconds` and `delta_rows` asks for: every
/// selected bar goes the same distance in time and the same number of
/// tracks, and lands among the bars it did not travel with.
fn drag_moves(
    rows: &[TimelineRow],
    selection: &BTreeSet<BarRef>,
    delta_seconds: f64,
    delta_rows: isize,
) -> Vec<BarMove> {
    let row_of = |id: u64| rows.iter().position(|row| row.track_id == id);
    let selected_rows: Vec<usize> = selection
        .iter()
        .filter_map(|bar| row_of(bar.track_id))
        .collect();
    let (Some(&first), Some(&last)) = (selected_rows.iter().min(), selected_rows.iter().max())
    else {
        return Vec::new();
    };
    let delta_rows = delta_rows.clamp(-(first as isize), (rows.len() - 1 - last) as isize);
    if delta_seconds == 0.0 && delta_rows == 0 {
        return Vec::new();
    }
    // Consecutive selected bars of one track travel as one block: the block
    // lands where its centre lands, so bars that belong together stay
    // together and keep their order.
    let mut blocks: Vec<(f64, usize, Vec<BarRef>)> = Vec::new();
    for (row, bars) in rows.iter().enumerate() {
        let mut run: Vec<&TimelineBar> = Vec::new();
        let mut flush = |run: &mut Vec<&TimelineBar>| {
            if let (Some(first), Some(last)) = (run.first(), run.last()) {
                let refs = run
                    .iter()
                    .map(|block| BarRef { track_id: bars.track_id, bar: block.bar })
                    .collect();
                blocks.push(((first.from + last.to) / 2.0, row, refs));
            }
            run.clear();
        };
        for block in bars.bars.iter().filter(|block| !block.repeated) {
            let chosen = selection.contains(&BarRef { track_id: bars.track_id, bar: block.bar });
            if chosen {
                run.push(block);
            } else {
                flush(&mut run);
            }
        }
        flush(&mut run);
    }
    blocks.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    blocks
        .into_iter()
        .flat_map(|(center, row, bars)| {
            let destination = &rows[(row as isize + delta_rows) as usize];
            let landing = center + delta_seconds;
            let insert_before = destination
                .bars
                .iter()
                .filter(|block| {
                    !block.repeated
                        && !selection.contains(&BarRef {
                            track_id: destination.track_id,
                            bar: block.bar,
                        })
                })
                .filter(|block| (block.from + block.to) / 2.0 < landing)
                .count();
            bars.into_iter().map(move |from| BarMove {
                from,
                to_track: destination.track_id,
                insert_before,
            })
        })
        .collect()
}

/// Second on the destination row where a moved bar will be inserted.
fn insertion_time(rows: &[TimelineRow], selection: &BTreeSet<BarRef>, bar_move: &BarMove) -> f64 {
    let Some(row) = rows.iter().find(|row| row.track_id == bar_move.to_track) else {
        return 0.0;
    };
    let remaining: Vec<&TimelineBar> = row
        .bars
        .iter()
        .filter(|block| {
            !block.repeated
                && !selection.contains(&BarRef {
                    track_id: row.track_id,
                    bar: block.bar,
                })
        })
        .collect();
    match remaining.get(bar_move.insert_before) {
        Some(block) => block.from,
        None => remaining.last().map_or(0.0, |block| block.to),
    }
}

/// A length in quarter notes, without trailing zeros.
fn beats(ticks: Tick) -> String {
    let quarters = ticks.abs() as f64 / TICKS_PER_QUARTER as f64;
    let text = format!("{quarters:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// How one selected bar sits against the one under the pointer.
struct BarComparison {
    other: BarRef,
    /// Start of the other bar minus start of the hovered one, in ticks of
    /// each track's own axis and in seconds.
    offset: Tick,
    offset_seconds: f64,
    /// Length of the other bar minus length of the hovered one.
    size: Tick,
    size_seconds: f64,
    /// The two bars run at different speeds, so ticks and seconds disagree.
    tempo_differs: bool,
    /// When the bar lines of both tracks meet again; `None` for bars that
    /// already run in step.
    resync: Option<ResyncAfter>,
}

struct ResyncAfter {
    result: Resync,
    /// The meeting lies past the last written bar of one of the tracks.
    past_end: bool,
}

/// Differences below this many seconds are rounding, not a real offset.
const SAME_TIME: f64 = 1e-6;

impl BarComparison {
    fn offset_label(&self) -> String {
        let sign = if self.offset_seconds < 0.0 { '-' } else { '+' };
        if self.tempo_differs {
            format!("{sign}{}", seconds_text(self.offset_seconds))
        } else {
            format!("{sign}{}", beats(self.offset))
        }
    }

    fn describe(&self, name: &str) -> String {
        let amount = |ticks: Tick, seconds: f64| {
            if self.tempo_differs {
                format!("{} ({:+} ticks)", seconds_text(seconds), ticks)
            } else {
                beat_count(ticks)
            }
        };
        let position = if self.offset_seconds.abs() < SAME_TIME {
            "starts together".to_owned()
        } else if self.offset_seconds > 0.0 {
            format!("{} behind", amount(self.offset, self.offset_seconds))
        } else {
            format!("{} ahead", amount(self.offset, self.offset_seconds))
        };
        let size = if self.size_seconds.abs() < SAME_TIME {
            "same length".to_owned()
        } else if self.size_seconds > 0.0 {
            format!("{} longer", amount(self.size, self.size_seconds))
        } else {
            format!("{} shorter", amount(self.size, self.size_seconds))
        };
        let mut text = format!("{name} bar {}: {position}, {size}", self.other.bar + 1);
        if let Some(after) = &self.resync {
            let bars = |count: usize| if count == 1 { "1 bar".to_owned() } else { format!("{count} bars") };
            match after.result {
                Resync::After { first, second } => {
                    text.push_str(&format!(
                        "\n    in step again after {} here, {} there{}",
                        bars(first),
                        bars(second),
                        if after.past_end { " (past the last bar)" } else { "" }
                    ));
                }
                Resync::Never => text.push_str("\n    never back in step"),
                Resync::Unknown => text.push_str("\n    step cannot be worked out (tempo ramp)"),
            }
        }
        text
    }
}

/// A length in seconds with two decimals, without trailing zeros.
fn seconds_text(seconds: f64) -> String {
    let text = format!("{:.2}", seconds.abs());
    format!("{} s", text.trim_end_matches('0').trim_end_matches('.'))
}

/// "1 beat" or "2.5 beats".
fn beat_count(ticks: Tick) -> String {
    let text = beats(ticks);
    if text == "1" {
        "1 beat".to_owned()
    } else {
        format!("{text} beats")
    }
}

/// The bars of `row` from `index` on as a run, when all of them are exact.
fn run_from(row: &TimelineRow, index: usize) -> Option<BarRun> {
    let start = row.bars.get(index)?.exact_from?;
    let lengths = row.bars[index..]
        .iter()
        .map(|block| block.exact_length)
        .collect::<Option<Vec<_>>>()?;
    Some(BarRun { start, lengths })
}

fn resync_of(base_row: &TimelineRow, base: &TimelineBar, other_row: &TimelineRow, other: &TimelineBar) -> Option<ResyncAfter> {
    let same_start = (other.from - base.from).abs() < SAME_TIME;
    let same_length = ((other.to - other.from) - (base.to - base.from)).abs() < SAME_TIME;
    if same_start && same_length {
        return None;
    }
    let position = |row: &TimelineRow, block: &TimelineBar| {
        row.bars.iter().position(|candidate| std::ptr::eq(candidate, block))
    };
    let runs = position(base_row, base)
        .zip(position(other_row, other))
        .and_then(|(a, b)| run_from(base_row, a).zip(run_from(other_row, b)));
    let Some((here, there)) = runs else {
        return Some(ResyncAfter {
            result: Resync::Unknown,
            past_end: false,
        });
    };
    let result = resync(&here, &there);
    let past_end = matches!(result, Resync::After { first, second }
        if first > here.lengths.len() || second > there.lengths.len());
    Some(ResyncAfter { result, past_end })
}

fn compare_to(
    rows: &[TimelineRow],
    selection: &BTreeSet<BarRef>,
    hovered: BarRef,
) -> Vec<BarComparison> {
    let Some(base_row) = rows.iter().find(|row| row.track_id == hovered.track_id) else {
        return Vec::new();
    };
    let Some(base) = base_row.original(hovered.bar) else {
        return Vec::new();
    };
    let speed = |block: &TimelineBar| (block.to - block.from) / (block.end - block.start).max(1) as f64;
    selection
        .iter()
        .filter(|other| other.track_id != hovered.track_id)
        .filter_map(|&other| {
            let other_row = rows.iter().find(|row| row.track_id == other.track_id)?;
            let block = other_row.original(other.bar)?;
            Some(BarComparison {
                other,
                offset: block.start - base.start,
                offset_seconds: block.from - base.from,
                size: (block.end - block.start) - (base.end - base.start),
                size_seconds: (block.to - block.from) - (base.to - base.from),
                tempo_differs: (speed(block) - speed(base)).abs() > 1e-9,
                resync: resync_of(base_row, base, other_row, block),
            })
        })
        .collect()
}

/// How a block is drawn.
#[derive(Clone, Copy, Default)]
struct BlockLook {
    row_selected: bool,
    selected: bool,
    cursor_here: bool,
    playing_here: bool,
    hovered: bool,
    warning: bool,
    ghost: bool,
}

impl BetterWriterApp {
    /// Draws the timeline and handles its mouse input. Returns the vertical
    /// scroll offset the track table follows.
    pub(super) fn show_bar_timeline(
        &mut self,
        ui: &mut egui::Ui,
        width: f32,
        height: f32,
        scroll_y: f32,
    ) -> f32 {
        if width < TIMELINE_MIN_WIDTH {
            return scroll_y;
        }
        let rows: Vec<TimelineRow> = self
            .project
            .tracks
            .iter()
            .map(|track| TimelineRow::new(track, &self.project.tempo_map_for(track.id)))
            .collect();
        // Edits elsewhere (undo, deleted bars or tracks) can leave selected
        // bars that are no longer there.
        self.timeline_selection.retain(|bar| {
            rows.iter()
                .find(|row| row.track_id == bar.track_id)
                .is_some_and(|row| row.original(bar.bar).is_some())
        });
        if self
            .timeline_anchor
            .is_some_and(|anchor| !self.timeline_selection.contains(&anchor))
        {
            self.timeline_anchor = self.timeline_selection.iter().next().copied();
        }
        let scale = timeline_scale(&rows, width);
        let content_width = rows
            .iter()
            .filter_map(|row| row.bars.last())
            .map(|bar| bar.to as f32 * scale)
            .fold(0.0_f32, f32::max);
        let playing = self.playback_seconds;
        let modifiers = ui.input(|input| input.modifiers);
        let pointer_down = ui.input(|input| input.pointer.latest_pos());
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.timeline_drag = None;
        }

        let area = egui::ScrollArea::both()
            .id_salt("bar_timeline")
            .max_width(width)
            .max_height(height)
            .auto_shrink([false, false])
            .scroll_source(
                egui::scroll_area::ScrollSource::SCROLL_BAR | egui::scroll_area::ScrollSource::MOUSE_WHEEL,
            )
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .vertical_scroll_offset(scroll_y)
            .show(ui, |ui| ui.vertical(|ui| {
                let origin = ui.cursor().min;
                let pitch = TRACK_ROW_HEIGHT + ui.spacing().item_spacing.y;
                let cell_at = |pos: Pos2| -> Option<(usize, &TimelineBar)> {
                    let y = pos.y - origin.y;
                    let row = (y / pitch).floor();
                    if row < 0.0 || y - row * pitch > TRACK_ROW_HEIGHT {
                        return None;
                    }
                    let row = row as usize;
                    let block = rows
                        .get(row)?
                        .block_at(f64::from((pos.x - origin.x) / scale))?;
                    Some((row, block))
                };
                let bar_ref = |row: usize, block: &TimelineBar| BarRef {
                    track_id: rows[row].track_id,
                    bar: block.bar,
                };

                let dragging = self.timeline_drag.filter(TimelineDrag::is_moving);
                let hovered = if dragging.is_none() && ui.clip_rect().contains(pointer_down.unwrap_or(Pos2::ZERO)) {
                    pointer_down.and_then(cell_at).map(|(row, block)| (row, bar_ref(row, block)))
                } else {
                    None
                };

                // What a drag in progress would do, shown before it happens.
                let mut moves: Vec<BarMove> = Vec::new();
                let mut warned: BTreeSet<BarRef> = BTreeSet::new();
                if let Some(drag) = dragging {
                    let delta = drag.pointer - drag.press;
                    moves = drag_moves(
                        &rows,
                        &self.timeline_selection,
                        f64::from(delta.x / scale),
                        (delta.y / pitch).round() as isize,
                    );
                    if !moves.is_empty() {
                        let mut scratch = self.project.clone();
                        if let Some(report) = scratch.move_bars(&moves) {
                            warned.extend(report.broken_ties);
                        }
                    }
                }

                let comparisons = hovered
                    .filter(|(_, bar)| self.timeline_selection.contains(bar))
                    .map(|(_, bar)| (bar, compare_to(&rows, &self.timeline_selection, bar)))
                    .filter(|(_, list)| !list.is_empty());
                let mut clicked_cell: Option<(usize, Option<usize>)> = None;
                let mut drag_started: Option<BarRef> = None;
                let mut drag_stopped = false;

                for (row_index, row) in rows.iter().enumerate() {
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(content_width.max(width), TRACK_ROW_HEIGHT),
                        Sense::click_and_drag(),
                    );
                    let row_selected = row.track_id == self.selected_track_id;
                    for block in &row.bars {
                        let bar = BarRef {
                            track_id: row.track_id,
                            bar: block.bar,
                        };
                        let rect_of = |shift_x: f32| {
                            Rect::from_min_max(
                                Pos2::new(
                                    rect.left() + block.from as f32 * scale + shift_x,
                                    rect.top() + 2.0,
                                ),
                                Pos2::new(
                                    rect.left() + block.to as f32 * scale - 1.0 + shift_x,
                                    rect.bottom() - 2.0,
                                ),
                            )
                        };
                        let look = BlockLook {
                            row_selected,
                            selected: self.timeline_selection.contains(&bar),
                            cursor_here: row_selected
                                && !block.repeated
                                && self.cursor.tick >= block.score_start
                                && self.cursor.tick < block.score_start + (block.end - block.start),
                            playing_here: playing
                                .is_some_and(|time| time >= block.from && time < block.to),
                            hovered: hovered.is_some_and(|(_, over)| over == bar),
                            warning: warned.contains(&bar) && !block.repeated,
                            ghost: false,
                        };
                        self.paint_timeline_block(ui.painter(), rect_of(0.0), block, row, look);
                    }

                    if let Some(comparisons) = &comparisons {
                        let (base, list) = comparisons;
                        let base_start = rows
                            .iter()
                            .find(|candidate| candidate.track_id == base.track_id)
                            .and_then(|candidate| candidate.original(base.bar))
                            .map_or(0.0, |block| block.from);
                        let x = rect.left() + base_start as f32 * scale;
                        ui.painter().line_segment(
                            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                            Stroke::new(1.0_f32, self.theme.highlight_color),
                        );
                        if let Some(entry) = list.iter().find(|entry| entry.other.track_id == row.track_id)
                            && let Some(block) = row.original(entry.other.bar)
                        {
                            let left = rect.left() + block.from as f32 * scale;
                            let right = rect.left() + block.to as f32 * scale;
                            ui.painter().text(
                                Pos2::new((left + right) * 0.5, rect.center().y),
                                Align2::CENTER_CENTER,
                                entry.offset_label(),
                                FontId::monospace(11.0),
                                self.theme.highlight_color,
                            );
                        }
                    }

                    if dragging.is_some() {
                        for bar_move in &moves {
                            if bar_move.to_track == row.track_id {
                                let x = rect.left()
                                    + insertion_time(&rows, &self.timeline_selection, bar_move) as f32 * scale;
                                ui.painter().line_segment(
                                    [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                                    Stroke::new(2.0_f32, self.theme.highlight_color),
                                );
                            }
                        }
                    }

                    if response.drag_started()
                        && let Some(press) = ui.input(|input| input.pointer.press_origin())
                        && let Some((hit_row, block)) = cell_at(press)
                        && hit_row == row_index
                    {
                        drag_started = Some(bar_ref(hit_row, block));
                    }
                    if response.clicked()
                        && let Some(pos) = response.interact_pointer_pos()
                    {
                        clicked_cell = Some((row_index, cell_at(pos).map(|(_, block)| block.bar)));
                    }
                    if response.drag_stopped() {
                        drag_stopped = true;
                    }
                    if let Some((over_row, over)) = hovered
                        && over_row == row_index
                    {
                        let name = &row.name;
                        let text = match &comparisons {
                            Some((_, list)) => {
                                let mut lines =
                                    vec![format!("{name} bar {}: compared with", over.bar + 1)];
                                for entry in list {
                                    let other_name = rows
                                        .iter()
                                        .find(|candidate| candidate.track_id == entry.other.track_id)
                                        .map_or("", |candidate| candidate.name.as_str());
                                    lines.push(entry.describe(other_name));
                                }
                                lines.join("\n")
                            }
                            None => {
                                let signature = pointer_down
                                    .and_then(cell_at)
                                    .map(|(_, block)| (block.signature, block.repeated));
                                match signature {
                                    Some((signature, repeated)) => format!(
                                        "{name} - bar {}, {}/{}{}",
                                        over.bar + 1,
                                        signature.numerator,
                                        signature.denominator,
                                        if repeated { " (repeat)" } else { "" }
                                    ),
                                    None => name.clone(),
                                }
                            }
                        };
                        response.clone().on_hover_text(text);
                    }
                }

                // Ghosts of the bars being carried, at the pointer.
                if let Some(drag) = dragging {
                    let delta = drag.pointer - drag.press;
                    for &bar in &self.timeline_selection {
                        let Some(row_index) = rows.iter().position(|row| row.track_id == bar.track_id)
                        else {
                            continue;
                        };
                        let Some(block) = rows[row_index].original(bar.bar) else {
                            continue;
                        };
                        let shifted_row = (row_index as isize + (delta.y / pitch).round() as isize)
                            .clamp(0, rows.len() as isize - 1);
                        let top = origin.y + shifted_row as f32 * pitch + 2.0;
                        let ghost = Rect::from_min_max(
                            Pos2::new(origin.x + block.from as f32 * scale + delta.x, top),
                            Pos2::new(
                                origin.x + block.to as f32 * scale - 1.0 + delta.x,
                                top + TRACK_ROW_HEIGHT - 4.0,
                            ),
                        );
                        let look = BlockLook {
                            selected: true,
                            ghost: true,
                            warning: warned.contains(&bar),
                            ..BlockLook::default()
                        };
                        self.paint_timeline_block(ui.painter(), ghost, block, &rows[row_index], look);
                    }
                }

                (rows, drag_started, clicked_cell, drag_stopped)
            }).inner);
        let (rows, drag_started, clicked_cell, drag_stopped) = area.inner;

        if let Some(pointer) = pointer_down
            && let Some(drag) = &mut self.timeline_drag
        {
            drag.pointer = pointer;
        }
        if let Some(grabbed) = drag_started
            && let Some(press) = ui.input(|input| input.pointer.press_origin())
        {
            if !self.timeline_selection.contains(&grabbed) {
                if !modifiers.command {
                    self.timeline_selection.clear();
                }
                let group = self.loop_group(grabbed);
                self.timeline_selection.extend(group);
                self.timeline_anchor = Some(grabbed);
            }
            self.timeline_drag = Some(TimelineDrag {
                press,
                pointer: press,
            });
        }
        if let Some((row_index, bar)) = clicked_cell {
            self.click_timeline(&rows, row_index, bar, modifiers);
        }
        if drag_stopped
            && let Some(drag) = self.timeline_drag.take()
            && drag.is_moving()
        {
            let delta = drag.pointer - drag.press;
            let moves = drag_moves(
                &rows,
                &self.timeline_selection,
                f64::from(delta.x / timeline_scale(&rows, width)),
                (delta.y / (TRACK_ROW_HEIGHT + ui.spacing().item_spacing.y)).round() as isize,
            );
            self.apply_bar_moves(&moves);
        }
        area.state.offset.y
    }

    fn click_timeline(
        &mut self,
        rows: &[TimelineRow],
        row_index: usize,
        bar: Option<usize>,
        modifiers: egui::Modifiers,
    ) {
        let Some(bar) = bar else {
            if !modifiers.command && !modifiers.shift {
                self.timeline_selection.clear();
            }
            return;
        };
        let track_id = rows[row_index].track_id;
        let target = BarRef { track_id, bar };
        let group = self.loop_group(target);
        if modifiers.command {
            if self.timeline_selection.contains(&target) {
                for bar in &group {
                    self.timeline_selection.remove(bar);
                }
            } else {
                self.timeline_selection.extend(group);
            }
            self.timeline_anchor = Some(target);
            return;
        }
        if modifiers.shift
            && let Some(anchor) = self.timeline_anchor
        {
            self.timeline_selection = range_between(rows, anchor, target).into_iter().collect();
            return;
        }
        self.timeline_selection = group.into_iter().collect();
        self.timeline_anchor = Some(target);
        if let Some(block) = rows[row_index].original(bar) {
            self.select_bar(track_id, block.score_start);
        }
    }

    /// A bar and the rest of its repeated region: selecting one bar of a
    /// loop of several bars takes them all.
    fn loop_group(&self, bar: BarRef) -> Vec<BarRef> {
        self.project.track(bar.track_id).map_or_else(
            || vec![bar],
            |track| {
                track
                    .loop_bars(bar.bar)
                    .into_iter()
                    .map(|index| BarRef { track_id: bar.track_id, bar: index })
                    .collect()
            },
        )
    }

    /// Carries out a bar drag as one undo step.
    fn apply_bar_moves(&mut self, moves: &[BarMove]) {
        if moves.is_empty() {
            return;
        }
        let before = self.project.clone();
        let Some(report) = self.project.move_bars(moves) else {
            self.project = before;
            return;
        };
        if self.project == before {
            return;
        }
        let touched: Vec<u64> = moves
            .iter()
            .flat_map(|bar_move| [bar_move.from.track_id, bar_move.to_track])
            .collect::<BTreeSet<u64>>()
            .into_iter()
            .collect();
        self.record_project_edit(before);
        self.normalize_for_view_mode();
        for track_id in touched {
            self.refresh_after_edit(track_id);
        }
        self.timeline_selection = report.landed.iter().map(|&(_, now)| now).collect();
        self.timeline_anchor = self.timeline_selection.iter().next().copied();
        self.prune_selection();
        let mut status = format!("Moved {} bar(s).", moves.len());
        if !report.broken_ties.is_empty() {
            status.push_str(" A tied note was broken at the bar edge.");
        }
        if report.clamped_notes > 0 {
            status.push_str(&format!(
                " {} note(s) moved to the lowest string of a smaller instrument.",
                report.clamped_notes
            ));
        }
        self.status = status;
    }

    fn paint_timeline_block(
        &self,
        painter: &egui::Painter,
        block_rect: Rect,
        block: &TimelineBar,
        row: &TimelineRow,
        look: BlockLook,
    ) {
        let base = if block.repeated {
            self.theme.accent2
        } else {
            self.theme.timeline_color
        };
        let [ar, ag, ab, _] = base.to_array();
        let [wr, wg, wb, _] = self.theme.warning_color.to_array();
        let alpha = |value: u8| egui::Color32::from_rgba_unmultiplied(ar, ag, ab, value);
        let mut fill = match (block.has_notes, look.row_selected) {
            (true, true) => alpha(120),
            (true, false) => alpha(60),
            (false, true) if block.repeated => alpha(70),
            (false, false) if block.repeated => alpha(40),
            (false, true) => self.theme.sheet_background,
            (false, false) => self.theme.panel_background,
        };
        if look.selected {
            fill = alpha(if look.ghost { 140 } else { 190 });
        }
        if look.warning {
            fill = egui::Color32::from_rgba_unmultiplied(wr, wg, wb, if look.ghost { 150 } else { 200 });
        }
        painter.rect_filled(block_rect, 2.0, fill);
        let outline = if look.warning {
            Some(Stroke::new(2.0_f32, self.theme.warning_color))
        } else if look.selected {
            Some(Stroke::new(2.0_f32, base))
        } else if look.playing_here {
            Some(Stroke::new(1.5_f32, self.theme.highlight_color))
        } else if look.cursor_here {
            Some(Stroke::new(1.5_f32, base))
        } else if look.hovered {
            Some(Stroke::new(1.0_f32, self.theme.muted_foreground))
        } else {
            None
        };
        if let Some(stroke) = outline {
            painter.rect_stroke(block_rect, 2.0, stroke, StrokeKind::Inside);
        }
        let painter = painter.with_clip_rect(block_rect);
        painter.text(
            block_rect.left_center() + Vec2::new(4.0, 0.0),
            Align2::LEFT_CENTER,
            (block.bar + 1).to_string(),
            FontId::monospace(9.0),
            self.theme.muted_foreground,
        );
        let previous = row
            .bars
            .iter()
            .position(|other| std::ptr::eq(other, block))
            .and_then(|index| index.checked_sub(1))
            .map(|index| &row.bars[index]);
        let signature_changed = previous.is_none_or(|before| before.signature != block.signature);
        if signature_changed && !look.ghost && block_rect.width() >= TIMELINE_LABELLED_WIDTH {
            painter.text(
                block_rect.right_center() - Vec2::new(4.0, 0.0),
                Align2::RIGHT_CENTER,
                format!("{}/{}", block.signature.numerator, block.signature.denominator),
                FontId::monospace(9.0),
                self.theme.notation_foreground,
            );
        }
    }

    /// Moves the edit cursor to the start of a bar of `track_id`, selecting
    /// the track.
    pub(super) fn select_bar(&mut self, track_id: u64, tick: Tick) {
        if track_id != self.selected_track_id {
            self.selected_track_id = track_id;
            self.selected_note_id = None;
            self.area_selected_note_ids.clear();
            self.reset_cursor_for_track(track_id);
        }
        self.cursor.tick = tick;
        self.cursor.pending_fret = None;
        self.cursor.pending_at = None;
        self.sync_time_signature_picker();
        self.reveal_cursor = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TempoPoint;

    fn timeline_row(bar_ticks: &[Tick]) -> TimelineRow {
        timeline_row_for(1, bar_ticks)
    }

    /// Seconds that `ticks` last at 120 bpm.
    fn secs(ticks: Tick) -> f64 {
        ticks as f64 / (2 * TICKS_PER_QUARTER) as f64
    }

    fn timeline_row_for(track_id: u64, bar_ticks: &[Tick]) -> TimelineRow {
        timeline_row_at(track_id, 120.0, bar_ticks)
    }

    fn timeline_row_at(track_id: u64, bpm: f32, bar_ticks: &[Tick]) -> TimelineRow {
        let tempo = TempoMap::new(&[TempoPoint::new(0, bpm)]);
        let mut start = 0;
        let mut exact_from = Some((0, 1));
        let bars = bar_ticks
            .iter()
            .enumerate()
            .map(|(bar, &length)| {
                let exact_length = tempo.exact_seconds(start, start + length);
                let block_from = exact_from;
                exact_from = exact_from
                    .zip(exact_length)
                    .and_then(|(from, length)| add_fractions(from, length));
                let block = TimelineBar {
                    start,
                    end: start + length,
                    from: tempo.seconds_at(start as f64),
                    to: tempo.seconds_at((start + length) as f64),
                    exact_from: block_from,
                    exact_length,
                    signature: TimeSignature {
                        numerator: 4,
                        denominator: 4,
                    },
                    has_notes: false,
                    bar,
                    score_start: start,
                    repeated: false,
                };
                start += length;
                block
            })
            .collect();
        TimelineRow {
            track_id,
            name: String::new(),
            bars,
        }
    }

    fn selection(bars: &[(u64, usize)]) -> BTreeSet<BarRef> {
        bars.iter()
            .map(|&(track_id, bar)| BarRef { track_id, bar })
            .collect()
    }

    #[test]
    fn timeline_blocks_fill_the_width_up_to_a_cap() {
        let q = TICKS_PER_QUARTER;
        let rows = [timeline_row(&[q * 4; 8])];
        let scale = timeline_scale(&rows, 1_000.0);
        assert!((scale * secs(q * 4 * 8) as f32 - 1_000.0).abs() < 1.0);

        let few = [timeline_row(&[q * 4; 2])];
        assert_eq!(timeline_scale(&few, 1_000.0), TIMELINE_MAX_SCALE);
    }

    #[test]
    fn the_shortest_bar_keeps_a_clickable_width() {
        let q = TICKS_PER_QUARTER;
        let rows = [timeline_row(&[q * 4; 200]), timeline_row(&[q; 50])];
        let scale = timeline_scale(&rows, 400.0);
        assert!((scale * secs(q) as f32 - TIMELINE_MIN_BLOCK).abs() < 0.01);
        assert!(scale * secs(q * 4) as f32 > TIMELINE_MIN_BLOCK * 3.9);
    }

    #[test]
    fn timeline_rows_follow_each_tracks_bars() {
        let project = BwxProject::demo();
        let rows: Vec<TimelineRow> = project
            .tracks
            .iter()
            .map(|track| TimelineRow::new(track, &project.tempo_map_for(track.id)))
            .collect();
        assert_eq!(rows[0].bars.len(), project.tracks[0].bar_count as usize);
        assert!(rows[0].bars[0].has_notes);
        assert!(!rows[0].bars[3].has_notes);
        let ticks: Vec<Tick> = rows[1].bars.iter().map(|bar| bar.end - bar.start).collect();
        assert!(ticks.iter().all(|&length| length == TICKS_PER_QUARTER * 5 / 2));
    }

    #[test]
    fn a_repeated_region_adds_copies_that_name_the_bars_they_copy() {
        let mut project = BwxProject::demo();
        let bar = project.tracks[0].bar_spans()[1].start;
        project.tracks[0].set_repeat_open(bar, true);
        project.tracks[0].set_repeat_close(bar, 1);
        let row = TimelineRow::new(&project.tracks[0], &project.tempo_map_for(project.tracks[0].id));
        let plays: Vec<(usize, bool)> = row.bars.iter().map(|b| (b.bar, b.repeated)).collect();
        assert_eq!(plays, [(0, false), (1, false), (1, true), (2, false), (3, false)]);
        assert_eq!(row.original(1).unwrap().start, row.bars[1].start);
    }

    #[test]
    fn shift_selects_a_run_on_one_track_and_a_time_range_across_tracks() {
        let q = TICKS_PER_QUARTER * 4;
        let rows = [timeline_row_for(1, &[q; 4]), timeline_row_for(2, &[q; 4])];
        let run = range_between(
            &rows,
            BarRef { track_id: 1, bar: 3 },
            BarRef { track_id: 1, bar: 1 },
        );
        assert_eq!(run.len(), 3);
        let block = range_between(
            &rows,
            BarRef { track_id: 1, bar: 1 },
            BarRef { track_id: 2, bar: 2 },
        );
        assert_eq!(block.len(), 4);
        assert!(block.contains(&BarRef { track_id: 2, bar: 1 }));
    }

    #[test]
    fn dragging_a_bar_past_its_neighbour_inserts_it_after_that_neighbour() {
        let q = TICKS_PER_QUARTER * 4;
        let rows = [timeline_row(&[q; 4])];
        // Centre of bar 0 plus one and a half bars lands on the boundary
        // between bars 1 and 2, so the bar goes in right after bar 1.
        let moves = drag_moves(&rows, &selection(&[(1, 0)]), secs(q + q / 2), 0);
        assert_eq!(
            moves,
            [BarMove {
                from: BarRef { track_id: 1, bar: 0 },
                to_track: 1,
                insert_before: 1
            }]
        );
        let further = drag_moves(&rows, &selection(&[(1, 0)]), secs(q * 2 + q / 4), 0);
        assert_eq!(further[0].insert_before, 2);
        assert!(drag_moves(&rows, &selection(&[(1, 0)]), 0.0, 0).is_empty());
    }

    #[test]
    fn a_run_of_bars_lands_as_one_block_even_past_a_neighbour() {
        let q = TICKS_PER_QUARTER * 4;
        let rows = [timeline_row(&[q; 5])];
        let chosen = selection(&[(1, 0), (1, 1), (1, 2)]);
        // Bar 0 alone would land before bar 3 and bar 1 after it; as a block
        // the run goes in after bar 3 and bar 3 never sits between its bars.
        let moves = drag_moves(&rows, &chosen, secs(q * 2 + q / 4), 0);
        let inserts: Vec<usize> = moves.iter().map(|m| m.insert_before).collect();
        assert_eq!(inserts, [1, 1, 1]);
        let order: Vec<usize> = moves.iter().map(|m| m.from.bar).collect();
        assert_eq!(order, [0, 1, 2]);
    }

    #[test]
    fn dragging_down_moves_every_selected_bar_to_the_next_track_and_stops_at_the_last() {
        let q = TICKS_PER_QUARTER * 4;
        let rows = [
            timeline_row_for(1, &[q; 2]),
            timeline_row_for(2, &[q; 2]),
            timeline_row_for(3, &[q; 2]),
        ];
        let moves = drag_moves(&rows, &selection(&[(1, 0), (2, 1)]), 0.0, 5);
        let targets: Vec<u64> = moves.iter().map(|m| m.to_track).collect();
        assert_eq!(targets, [2, 3]);
    }

    #[test]
    fn bar_differences_read_as_ahead_behind_and_longer_shorter() {
        let q = TICKS_PER_QUARTER;
        let rows = [timeline_row_for(1, &[q * 4; 2]), timeline_row_for(2, &[q * 3; 3])];
        let picked = selection(&[(1, 1), (2, 1)]);
        let list = compare_to(&rows, &picked, BarRef { track_id: 1, bar: 1 });
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].offset, -q);
        assert_eq!(list[0].size, -q);
        assert_eq!(list[0].offset_label(), "-1");
        assert!(list[0].describe("B").contains("1 beat ahead"));
        assert_eq!(beats(q / 2), "0.5");
    }

    #[test]
    fn a_faster_track_gets_narrower_blocks_for_the_same_bars() {
        let q = TICKS_PER_QUARTER * 4;
        let rows = [timeline_row_at(1, 120.0, &[q; 2]), timeline_row_at(2, 240.0, &[q; 2])];
        let slow = rows[0].bars[0].to - rows[0].bars[0].from;
        let fast = rows[1].bars[0].to - rows[1].bars[0].from;
        assert!((slow - 2.0 * fast).abs() < 1e-9);
        assert!(rows[1].block_at(2.5).is_none());
        assert_eq!(rows[1].block_at(1.2).unwrap().bar, 1);
        assert_eq!(rows[0].block_at(1.2).unwrap().bar, 0);
    }

    #[test]
    fn different_tempos_report_seconds_and_the_tick_difference() {
        let q = TICKS_PER_QUARTER * 4;
        let rows = [timeline_row_at(1, 120.0, &[q; 3]), timeline_row_at(2, 240.0, &[q; 3])];
        let picked = selection(&[(1, 1), (2, 1)]);
        let list = compare_to(&rows, &picked, BarRef { track_id: 1, bar: 1 });
        assert!(list[0].tempo_differs);
        assert_eq!(list[0].offset, 0);
        assert!((list[0].offset_seconds + 1.0).abs() < 1e-9);
        let text = list[0].describe("B");
        assert!(text.contains("1 s (+0 ticks) ahead"), "{text}");
        assert!(text.contains("1 s (+0 ticks) shorter"), "{text}");
    }

    #[test]
    fn bars_at_different_speeds_report_when_they_line_up_again() {
        let q = TICKS_PER_QUARTER;
        // 2 s bars against 2.5 s bars: the lines meet at 10 s.
        let rows = [
            timeline_row_at(1, 120.0, &[q * 4; 12]),
            timeline_row_at(2, 120.0, &[q * 5; 12]),
        ];
        let picked = selection(&[(1, 0), (2, 0)]);
        let list = compare_to(&rows, &picked, BarRef { track_id: 1, bar: 0 });
        let text = list[0].describe("B");
        assert!(text.contains("in step again after 5 bars here, 4 bars there"), "{text}");
        assert!(!text.contains("past the last bar"));
    }

    #[test]
    fn a_meeting_beyond_the_written_bars_is_marked() {
        let q = TICKS_PER_QUARTER;
        let rows = [
            timeline_row_at(1, 120.0, &[q * 4; 3]),
            timeline_row_at(2, 120.0, &[q * 5; 3]),
        ];
        let picked = selection(&[(1, 0), (2, 0)]);
        let list = compare_to(&rows, &picked, BarRef { track_id: 1, bar: 0 });
        assert!(list[0].describe("B").contains("(past the last bar)"));
    }

    #[test]
    fn bars_in_step_say_nothing_about_resyncing() {
        let q = TICKS_PER_QUARTER * 4;
        let rows = [timeline_row_for(1, &[q; 2]), timeline_row_for(2, &[q; 2])];
        let picked = selection(&[(1, 0), (2, 0)]);
        let list = compare_to(&rows, &picked, BarRef { track_id: 1, bar: 0 });
        assert!(list[0].resync.is_none());
    }

    #[test]
    fn an_offset_that_never_closes_is_reported() {
        let q = TICKS_PER_QUARTER;
        let rows = [
            timeline_row_at(1, 120.0, &[q * 4; 4]),
            timeline_row_at(2, 120.0, &[q, q * 4, q * 4, q * 4]),
        ];
        let picked = selection(&[(1, 0), (2, 1)]);
        let list = compare_to(&rows, &picked, BarRef { track_id: 1, bar: 0 });
        assert!(list[0].describe("B").contains("never back in step"));
    }
}
