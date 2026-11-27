//! Bends and the tremolo bar in the editor: the window that edits one
//! envelope, the commands that open it, and the thin curves drawn over the
//! score for bent notes and dives.

use super::*;
use crate::core::pitch::{self, FULL_BEND, MAX_QUARTER_TONES, MIN_DIVE_TICKS, PitchEnvelope};

/// Points snap to this many ticks along the envelope (a sixteenth).
const POINT_SNAP: Tick = TICKS_PER_QUARTER / 4;
const GRAPH_HEIGHT: f32 = 190.0;
const GRAPH_PAD: f32 = 26.0;
const POINT_RADIUS: f32 = 5.0;
const HIT_RADIUS: f32 = 9.0;
/// Curve change per notch of the mouse wheel over a point.
const CURVE_STEP: i8 = 10;
/// Pixels a quarter tone is drawn tall over the score.
const STRIP_PX_PER_QUARTER_TONE: f32 = 2.5;

/// What the pitch window edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PitchTarget {
    /// The bend of the note with this id.
    Bend { note_id: u64 },
    /// The dive of the tremolo bar that starts at this tick.
    Dive { start: Tick },
}

#[derive(Clone, Debug)]
pub(super) struct PitchEditor {
    track_id: u64,
    target: PitchTarget,
    envelope: PitchEnvelope,
    /// Ticks the graph spans: the note's chain, or the dive's length.
    span: Tick,
    /// The longest a dive may be before it would run into the next one.
    max_span: Tick,
    /// Where a dive begins. Differs from the target's start once the user
    /// has moved it; Apply moves the dive there.
    start: Tick,
    selected: Option<usize>,
    dragging: Option<usize>,
    /// Quarter tones the graph shows, bottom and top.
    low: i16,
    high: i16,
    /// Size of the bend or dive the presets make, in quarter tones.
    amount: i16,
    /// Whether the envelope is already written in the project.
    exists: bool,
    title: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PitchAction {
    Apply,
    Remove,
    Close,
}

impl PitchEditor {
    fn is_dive(&self) -> bool {
        matches!(self.target, PitchTarget::Dive { .. })
    }

    /// Widens the graph so every point has room, never narrows it.
    fn fit_range(&mut self) {
        for point in &self.envelope.points {
            self.low = self.low.min(point.value - 2);
            self.high = self.high.max(point.value + 2);
        }
        self.low = self.low.max(-MAX_QUARTER_TONES - 2);
        self.high = self.high.min(MAX_QUARTER_TONES + 2);
    }

    fn gap(&self) -> Tick {
        POINT_SNAP.min((self.span / 8).max(1))
    }

    fn snap_time(&self, at: Tick) -> Tick {
        let grid = self.gap();
        ((at + grid / 2).div_euclid(grid) * grid).clamp(0, self.span)
    }

    /// Whether the point can be taken out: a dive keeps its two ends at rest
    /// and a bend keeps at least one point.
    fn can_remove(&self, index: usize) -> bool {
        let count = self.envelope.points.len();
        if self.is_dive() {
            index > 0 && index + 1 < count
        } else {
            count > 1
        }
    }

    /// The pins a dive's ends have: at rest, at their times.
    fn keep_dive_ends(&mut self) {
        if !self.is_dive() {
            return;
        }
        let span = self.span;
        let points = &mut self.envelope.points;
        if let Some(first) = points.first_mut() {
            first.at = 0;
            first.value = 0;
        }
        if points.len() > 1
            && let Some(last) = points.last_mut()
        {
            last.at = span;
            last.value = 0;
        }
    }

    /// Fills the envelope from a preset shape across the whole span.
    fn use_preset(&mut self, envelope: PitchEnvelope) {
        self.envelope = envelope;
        self.selected = None;
        self.keep_dive_ends();
        self.fit_range();
    }
}

/// What a bend of `quarter_tones` is called on a tab: a quarter tone is 1/4,
/// two are 1/2, four are a full bend.
pub(super) fn bend_label(quarter_tones: i16) -> String {
    let sign = if quarter_tones < 0 { "-" } else { "" };
    let (whole, rest) = (quarter_tones.abs() / 4, quarter_tones.abs() % 4);
    let fraction = ["", "1/4", "1/2", "3/4"][usize::from(rest.unsigned_abs())];
    let text = match (whole, rest) {
        (0, 0) => return "0".to_owned(),
        (1, 0) => "full".to_owned(),
        (0, _) => fraction.to_owned(),
        (_, 0) => whole.to_string(),
        _ => format!("{whole} {fraction}"),
    };
    format!("{sign}{text}")
}

fn beats(ticks: Tick) -> f64 {
    ticks as f64 / TICKS_PER_QUARTER as f64
}

impl BetterWriterApp {
    /// The first selected note with its chain: the note that carries the bend
    /// and how long the bend runs.
    fn note_to_bend(&self) -> Option<(u64, u64, Tick)> {
        let track = self.project.track(self.selected_track_id)?;
        let note = self
            .selected_note_ids()
            .into_iter()
            .find_map(|id| track.notes.iter().find(|note| note.id == id))?;
        let head = track.chain_head(note);
        Some((head.id, head.string_index as u64, track.sounding_end(head) - head.abs_tick))
    }

    /// Opens the window on the bend of the selected note, or on a new full
    /// bend for it.
    pub(super) fn open_bend_editor(&mut self) {
        let Some((note_id, string, length)) = self.note_to_bend() else {
            self.status = "Select a note to bend it.".to_owned();
            return;
        };
        let track_id = self.selected_track_id;
        let (fret, existing) = self
            .project
            .track(track_id)
            .and_then(|track| track.notes.iter().find(|note| note.id == note_id))
            .map_or((0, None), |note| (note.fret, note.effects.bend.clone()));
        let mut editor = PitchEditor {
            track_id,
            target: PitchTarget::Bend { note_id },
            envelope: existing.clone().unwrap_or_else(|| PitchEnvelope::bend(length, FULL_BEND)),
            span: length.max(1),
            max_span: length.max(1),
            start: 0,
            selected: None,
            dragging: None,
            low: -2,
            high: 12,
            amount: FULL_BEND,
            exists: existing.is_some(),
            title: format!("Bend of fret {fret} on string {}", string + 1),
        };
        editor.fit_range();
        self.pitch_editor = Some(editor);
    }

    /// Left-click on the Bend button: every selected note gets a full bend,
    /// or loses its bend when all of them already have one.
    pub(super) fn toggle_default_bend(&mut self) {
        let track_id = self.selected_track_id;
        let ids = self.selected_note_ids();
        let Some(track) = self.project.track(track_id) else {
            return;
        };
        let heads: Vec<(u64, Tick, bool)> = ids
            .iter()
            .filter_map(|id| track.notes.iter().find(|note| note.id == *id))
            .map(|note| track.chain_head(note))
            .map(|head| (head.id, track.sounding_end(head) - head.abs_tick, head.effects.bend.is_some()))
            .collect();
        if heads.is_empty() {
            self.status = "Select a note to bend it (right-click the button for the bend window).".to_owned();
            return;
        }
        let remove = heads.iter().all(|head| head.2);
        let before = self.project.clone();
        if let Some(track) = self.project.track_mut(track_id) {
            for (id, length, has_bend) in heads {
                if remove {
                    track.set_bend(id, None);
                } else if !has_bend {
                    track.set_bend(id, Some(PitchEnvelope::bend(length, FULL_BEND)));
                }
            }
        }
        self.status = if remove { "Bend removed.".to_owned() } else { "Full bend written.".to_owned() };
        self.record_project_edit(before);
        self.refresh_after_edit(track_id);
    }

    /// Opens the window on the tremolo bar dive under the cursor, or on a
    /// new dive starting there.
    pub(super) fn open_tremolo_editor(&mut self) {
        let track_id = self.selected_track_id;
        let tick = self.cursor.tick.max(0);
        let Some(track) = self.project.track(track_id) else {
            return;
        };
        let existing = track.dive_at(tick).map(|index| track.tremolo_bar[index].clone());
        let start = existing.as_ref().map_or(tick, |dive| dive.start);
        let next = track.tremolo_bar.iter().map(|dive| dive.start).filter(|&other| other > start).min();
        let length = existing
            .as_ref()
            .map_or_else(|| self.entry_ticks().max(MIN_DIVE_TICKS), |dive| dive.envelope.length());
        let room = next.map_or(Tick::MAX / 4, |next| next - start);
        if room < MIN_DIVE_TICKS {
            self.status = "There is no room for a tremolo bar dive here: the next one starts too soon.".to_owned();
            return;
        }
        let length = length.min(room);
        let mut editor = PitchEditor {
            track_id,
            target: PitchTarget::Dive { start },
            envelope: existing
                .as_ref()
                .map_or_else(|| PitchEnvelope::dive(length, FULL_BEND), |dive| dive.envelope.clone()),
            span: length,
            max_span: room.min(length.max(TICKS_PER_QUARTER) * 8),
            start,
            selected: None,
            dragging: None,
            low: -12,
            high: 4,
            amount: FULL_BEND,
            exists: existing.is_some(),
            title: format!("Tremolo bar dive from beat {:.2}", beats(start) + 1.0),
        };
        editor.fit_range();
        self.pitch_editor = Some(editor);
    }

    pub(super) fn show_pitch_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = &self.pitch_editor else {
            return;
        };
        let title = if editor.is_dive() { "Tremolo bar" } else { "Bend" };
        self.floating_window(
            ctx,
            floating::WindowSpec::new("pitch_editor_window", title, Vec2::new(520.0, 430.0), Vec2::new(420.0, 380.0)),
            |app, ui| app.pitch_editor_ui(ui),
            |app, _| app.pitch_editor = None,
        );
    }

    fn pitch_editor_ui(&mut self, ui: &mut egui::Ui) {
        let Some(mut editor) = self.pitch_editor.take() else {
            return;
        };
        let action = self.pitch_editor_contents(ui, &mut editor);
        match action {
            Some(PitchAction::Apply) => {
                self.pitch_editor = Some(editor);
                self.apply_pitch_editor();
            }
            Some(PitchAction::Remove) => {
                self.pitch_editor = Some(editor);
                self.remove_pitch_envelope();
                self.pitch_editor = None;
            }
            Some(PitchAction::Close) => {}
            None => self.pitch_editor = Some(editor),
        }
    }

    fn pitch_editor_contents(&mut self, ui: &mut egui::Ui, editor: &mut PitchEditor) -> Option<PitchAction> {
        let mut action = None;
        ui.label(egui::RichText::new(editor.title.clone()).strong());

        ui.horizontal_wrapped(|ui| {
            ui.label("Presets:");
            let (span, amount) = (editor.span, editor.amount);
            let presets: Vec<(&str, PitchEnvelope)> = if editor.is_dive() {
                vec![
                    ("Dive", PitchEnvelope::dive(span, amount)),
                    (
                        "Quick dive, slow return",
                        PitchEnvelope::new(vec![
                            pitch::PitchPoint::new(0, 0),
                            pitch::PitchPoint::new(span / 4, -amount),
                            pitch::PitchPoint::new(span, 0),
                        ]),
                    ),
                    (
                        "Slow dive, quick return",
                        PitchEnvelope::new(vec![
                            pitch::PitchPoint::new(0, 0),
                            pitch::PitchPoint::new(span * 3 / 4, -amount),
                            pitch::PitchPoint::new(span, 0),
                        ]),
                    ),
                ]
            } else {
                vec![
                    ("Bend", PitchEnvelope::bend(span, amount)),
                    ("Bend and release", PitchEnvelope::bend_release(span, amount)),
                    ("Pre-bend", PitchEnvelope::pre_bend(span, amount)),
                    ("Pre-bend and release", PitchEnvelope::pre_bend_release(span, amount)),
                ]
            };
            for (name, envelope) in presets {
                if ui.button(name).clicked() {
                    editor.use_preset(envelope);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Size of the presets");
            ui.add(
                egui::DragValue::new(&mut editor.amount)
                    .range(1..=MAX_QUARTER_TONES)
                    .custom_formatter(|value, _| bend_label(value as i16))
                    .custom_parser(|text| match text.trim() {
                        "full" => Some(f64::from(FULL_BEND)),
                        other => other.parse::<f64>().ok(),
                    }),
            );
            ui.weak(format!("({} quarter tones)", editor.amount));
        });
        if editor.is_dive() {
            ui.horizontal(|ui| {
                ui.label("Starts at beat");
                let mut at = beats(editor.start) + 1.0;
                let response = ui.add(egui::DragValue::new(&mut at).speed(0.02).range(1.0..=f64::MAX).fixed_decimals(2));
                if response.changed() {
                    editor.start = (((at - 1.0) * TICKS_PER_QUARTER as f64).round() as Tick).max(0);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Length");
                let mut length = beats(editor.span);
                let response = ui.add(
                    egui::DragValue::new(&mut length)
                        .speed(0.02)
                        .range(beats(MIN_DIVE_TICKS)..=beats(editor.max_span))
                        .suffix(" beats"),
                );
                if response.changed() {
                    let new_span = ((length * TICKS_PER_QUARTER as f64).round() as Tick).clamp(MIN_DIVE_TICKS, editor.max_span);
                    let old_span = editor.span.max(1);
                    for point in &mut editor.envelope.points {
                        point.at = point.at * new_span / old_span;
                    }
                    editor.span = new_span;
                    editor.keep_dive_ends();
                }
            });
        } else {
            ui.weak(format!("The bend runs along the note and its ties: {:.2} beats.", beats(editor.span)));
        }

        self.pitch_graph(ui, editor);
        ui.weak("Drag a point. Double-click to add one, right-click to remove it, scroll over it to curve the segment before it.");

        self.pitch_point_fields(ui, editor);

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                action = Some(PitchAction::Apply);
            }
            if editor.exists && ui.button(if editor.is_dive() { "Remove dive" } else { "Remove bend" }).clicked() {
                action = Some(PitchAction::Remove);
            }
            if ui.button("Close").clicked() {
                action = Some(PitchAction::Close);
            }
        });
        action
    }

    /// The fields of the point that is selected: its time, pitch and curve.
    fn pitch_point_fields(&mut self, ui: &mut egui::Ui, editor: &mut PitchEditor) {
        let Some(index) = editor.selected.filter(|&index| index < editor.envelope.points.len()) else {
            ui.weak("Click a point to type its time, pitch and curve.");
            return;
        };
        let point = editor.envelope.points[index];
        let last = index + 1 == editor.envelope.points.len();
        let dive = editor.is_dive();
        let time_enabled = index > 0 && !(dive && last);
        let value_enabled = !(dive && (index == 0 || last));
        let mut at = beats(point.at);
        let mut value = point.value;
        let mut curve = point.curve;
        ui.horizontal(|ui| {
            ui.label(format!("Point {}", index + 1));
            ui.add_enabled(
                time_enabled,
                egui::DragValue::new(&mut at).speed(0.01).range(0.0..=beats(editor.span)).suffix(" beats"),
            );
            ui.add_enabled(
                value_enabled,
                egui::DragValue::new(&mut value)
                    .range(-MAX_QUARTER_TONES..=MAX_QUARTER_TONES)
                    .custom_formatter(|value, _| bend_label(value as i16)),
            );
            ui.add_enabled(
                index > 0,
                egui::DragValue::new(&mut curve).range(-pitch::MAX_CURVE..=pitch::MAX_CURVE).prefix("curve "),
            );
            if editor.can_remove(index) && ui.button("Remove point").clicked() {
                editor.envelope.remove_point(index);
                editor.selected = None;
            }
        });
        if editor.selected == Some(index) && index < editor.envelope.points.len() {
            let gap = editor.gap();
            // Only a field the user changed moves the point: a point off the
            // snap grid stays where it is.
            if (at - beats(point.at)).abs() > 1e-6 || value != point.value {
                let new_at = ((at * TICKS_PER_QUARTER as f64).round() as Tick).clamp(0, editor.span);
                editor.envelope.move_point(index, new_at, value, gap, true);
                editor.keep_dive_ends();
                editor.fit_range();
            }
            if curve != point.curve {
                editor.envelope.set_curve(index, curve);
            }
        }
    }

    /// The graph: time across, pitch up, points to drag.
    fn pitch_graph(&mut self, ui: &mut egui::Ui, editor: &mut PitchEditor) {
        let width = ui.available_width().max(200.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, GRAPH_HEIGHT), Sense::click_and_drag());
        let plot = rect.shrink2(Vec2::new(GRAPH_PAD, 12.0));
        let (low, high) = (f32::from(editor.low), f32::from(editor.high));
        let span = editor.span.max(1) as f32;
        let to_screen = |at: Tick, value: f32| {
            Pos2::new(
                plot.left() + at as f32 / span * plot.width(),
                plot.bottom() - (value - low) / (high - low).max(1.0) * plot.height(),
            )
        };
        let from_screen = |pos: Pos2| {
            let at = ((pos.x - plot.left()) / plot.width().max(1.0) * span).round() as Tick;
            let value = low + (plot.bottom() - pos.y) / plot.height().max(1.0) * (high - low);
            (at, value.round() as i16)
        };
        let hit = |pos: Pos2, points: &[pitch::PitchPoint]| {
            points
                .iter()
                .enumerate()
                .map(|(index, point)| (index, to_screen(point.at, f32::from(point.value)).distance(pos)))
                .filter(|(_, distance)| *distance <= HIT_RADIUS)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(index, _)| index)
        };

        if let Some(pos) = response.hover_pos()
            && let Some(index) = hit(pos, &editor.envelope.points)
        {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 && index > 0 {
                let current = editor.envelope.points[index].curve;
                let step = if scroll > 0.0 { CURVE_STEP } else { -CURVE_STEP };
                editor.envelope.set_curve(index, current.saturating_add(step));
                editor.selected = Some(index);
                ui.input_mut(|input| input.smooth_scroll_delta = Vec2::ZERO);
            }
        }
        if response.drag_started()
            && let Some(pos) = response.interact_pointer_pos()
        {
            editor.dragging = hit(pos, &editor.envelope.points);
            if editor.dragging.is_some() {
                editor.selected = editor.dragging;
            }
        }
        if response.dragged()
            && let Some(index) = editor.dragging
            && let Some(pos) = response.interact_pointer_pos()
        {
            let (at, value) = from_screen(pos);
            let (at, gap) = (editor.snap_time(at), editor.gap());
            editor.envelope.move_point(index, at, value, gap, true);
            editor.keep_dive_ends();
        }
        if response.drag_stopped() {
            editor.dragging = None;
            editor.fit_range();
        }
        if response.double_clicked()
            && let Some(pos) = response.interact_pointer_pos()
            && hit(pos, &editor.envelope.points).is_none()
        {
            let (at, value) = from_screen(pos);
            let at = editor.snap_time(at);
            let inside = at > 0 && (!editor.is_dive() || at < editor.span);
            if inside {
                let index = editor.envelope.add_point(at, value);
                editor.selected = Some(index);
                editor.keep_dive_ends();
                editor.fit_range();
            }
        }
        if response.secondary_clicked()
            && let Some(pos) = response.interact_pointer_pos()
            && let Some(index) = hit(pos, &editor.envelope.points)
            && editor.can_remove(index)
        {
            editor.envelope.remove_point(index);
            editor.selected = None;
        }
        if response.hovered()
            && let Some(index) = editor.selected
            && ui.input(|input| input.key_pressed(egui::Key::Delete))
            && editor.can_remove(index)
        {
            editor.envelope.remove_point(index);
            editor.selected = None;
        }

        // Drawing.
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        let grid = Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color);
        let strong = Stroke::new(1.0_f32, ui.visuals().widgets.inactive.fg_stroke.color);
        let text_color = ui.visuals().weak_text_color();
        for value in editor.low..=editor.high {
            if value % 2 != 0 {
                continue;
            }
            let y = to_screen(0, f32::from(value)).y;
            let stroke = if value == 0 || value % 4 == 0 { strong } else { grid };
            painter.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], stroke);
            painter.text(
                Pos2::new(rect.left() + 3.0, y),
                Align2::LEFT_CENTER,
                bend_label(value),
                FontId::proportional(10.0),
                text_color,
            );
        }
        let mut beat = 0;
        while beat <= editor.span {
            let x = to_screen(beat, 0.0).x;
            painter.line_segment([Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())], grid);
            beat += TICKS_PER_QUARTER;
        }
        let line = Stroke::new(2.0_f32, self.theme.accent);
        let samples = 96;
        let curve: Vec<Pos2> = (0..=samples)
            .map(|step| {
                let at = editor.span * Tick::from(step) / Tick::from(samples);
                to_screen(at, editor.envelope.value_at(at))
            })
            .collect();
        painter.add(egui::Shape::line(curve, line));
        for (index, point) in editor.envelope.points.iter().enumerate() {
            let center = to_screen(point.at, f32::from(point.value));
            let selected = editor.selected == Some(index);
            painter.circle_filled(
                center,
                POINT_RADIUS,
                if selected { egui::Color32::WHITE } else { self.theme.accent },
            );
            painter.circle_stroke(center, POINT_RADIUS, Stroke::new(1.5_f32, self.theme.accent));
        }
    }

    /// Writes the window's envelope into the project.
    fn apply_pitch_editor(&mut self) {
        let Some(editor) = self.pitch_editor.clone() else {
            return;
        };
        let before = self.project.clone();
        let track_id = editor.track_id;
        let (done, message) = match editor.target {
            PitchTarget::Bend { note_id } => {
                let changed = self
                    .project
                    .track_mut(track_id)
                    .is_some_and(|track| track.set_bend(note_id, Some(editor.envelope.clone())));
                (true, if changed { "Bend written." } else { "The bend is as it was." })
            }
            PitchTarget::Dive { start } => match self.project.track_mut(track_id) {
                Some(track) => {
                    let placed = match track.tremolo_bar.iter().position(|dive| dive.start == start) {
                        Some(index) => {
                            let moved = editor.start == start || track.move_dive(index, editor.start);
                            moved
                                && track
                                    .tremolo_bar
                                    .iter()
                                    .position(|dive| dive.start == editor.start)
                                    .is_some_and(|index| track.set_dive(index, &editor.envelope))
                        }
                        None => track.add_dive(editor.start, &editor.envelope).is_some(),
                    };
                    (placed, if placed { "Tremolo bar dive written." } else { "That dive would run into another one." })
                }
                None => (false, "That track is gone."),
            },
        };
        self.status = message.to_owned();
        if !done {
            return;
        }
        if let Some(open) = self.pitch_editor.as_mut() {
            open.exists = true;
            if let PitchTarget::Dive { .. } = open.target {
                // The dive now begins where the window says.
                open.target = PitchTarget::Dive { start: open.start };
            }
        }
        self.record_project_edit(before);
        self.refresh_after_edit(track_id);
    }

    /// Takes the window's bend or dive out of the project.
    fn remove_pitch_envelope(&mut self) {
        let Some(editor) = self.pitch_editor.clone() else {
            return;
        };
        let before = self.project.clone();
        let track_id = editor.track_id;
        if let Some(track) = self.project.track_mut(track_id) {
            match editor.target {
                PitchTarget::Bend { note_id } => {
                    track.set_bend(note_id, None);
                }
                PitchTarget::Dive { start } => {
                    if let Some(index) = track.tremolo_bar.iter().position(|dive| dive.start == start) {
                        track.remove_dive(index);
                    }
                }
            }
        }
        self.status = "Removed.".to_owned();
        self.record_project_edit(before);
        self.refresh_after_edit(track_id);
    }

    /// Draws the pitch curves of the track's bent notes and tremolo bar dives
    /// that fall in `system`, over the strings: a bend rises from the line at
    /// `y`, a dive hangs from the line above it.
    pub(super) fn paint_pitch_envelopes(
        &self,
        painter: &egui::Painter,
        left: f32,
        y: f32,
        system: &StaffSystem,
        track: &InstrumentTrack,
    ) {
        let (start, end) = (system.start_tick, system.start_tick + system.tick_span());
        let x_at = |tick: Tick| {
            let tick = tick.clamp(start, end);
            if system.has_beat_layout() {
                self.content_x(system, left, tick)
            } else {
                left + system.x_of(tick, self.pixels_per_tick)
            }
        };
        let stroke = Stroke::new(1.5_f32, self.theme.accent);
        let weak = Stroke::new(1.0_f32, self.theme.muted_foreground);
        let draw = |from: Tick, to: Tick, base_y: f32, envelope: &PitchEnvelope| {
            let (from_x, to_x) = (x_at(from), x_at(to));
            if to_x - from_x < 2.0 {
                return;
            }
            painter.line_segment([Pos2::new(from_x, base_y), Pos2::new(to_x, base_y)], weak);
            let samples = (((to_x - from_x) / 5.0) as i32).clamp(6, 160);
            let shown_from = from.max(start);
            let shown_to = to.min(end);
            let points: Vec<Pos2> = (0..=samples)
                .map(|step| {
                    let tick = shown_from + (shown_to - shown_from) * Tick::from(step) / Tick::from(samples);
                    let value = envelope.value_at(tick - from);
                    Pos2::new(x_at(tick), base_y - value * STRIP_PX_PER_QUARTER_TONE)
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
            let peak = envelope.peak();
            if peak != 0 {
                let peak_tick = envelope
                    .points
                    .iter()
                    .find(|point| point.value == peak)
                    .map_or(0, |point| point.at);
                if (shown_from..=shown_to).contains(&(from + peak_tick)) {
                    painter.text(
                        // The label sits beyond the peak: above a bend, below a dive.
                        Pos2::new(
                            x_at(from + peak_tick),
                            base_y - (f32::from(peak) * STRIP_PX_PER_QUARTER_TONE + f32::from(peak.signum()) * 6.0),
                        ),
                        Align2::CENTER_CENTER,
                        bend_label(peak),
                        FontId::proportional(9.0),
                        self.theme.accent,
                    );
                }
            }
        };
        for note in &track.notes {
            let Some(bend) = &note.effects.bend else {
                continue;
            };
            let end_tick = track.sounding_end(note);
            if end_tick <= start || note.abs_tick >= end {
                continue;
            }
            draw(note.abs_tick, end_tick, y, bend);
        }
        for dive in &track.tremolo_bar {
            if dive.end() <= start || dive.start >= end {
                continue;
            }
            draw(dive.start, dive.end(), y - 26.0, &dive.envelope);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bends_are_named_the_way_a_tab_writes_them() {
        assert_eq!(bend_label(0), "0");
        assert_eq!(bend_label(1), "1/4");
        assert_eq!(bend_label(2), "1/2");
        assert_eq!(bend_label(3), "3/4");
        assert_eq!(bend_label(4), "full");
        assert_eq!(bend_label(6), "1 1/2");
        assert_eq!(bend_label(8), "2");
        assert_eq!(bend_label(-4), "-full");
    }
}
