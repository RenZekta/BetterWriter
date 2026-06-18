use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::{
    audio::{AudioRuntime, Vst3HostSlot, VstHost},
    core::{
        BwxProject, CompatibilityReport, DurationChoice, MidiEventKind, MidiPlaybackEvent,
        NoteEffects, ShadowTimeline, StandardExportFormat, TICKS_PER_QUARTER, Tick,
    },
    format,
    theme::EditorTheme,
};

/// The keyboard-driven write cursor. It points at a `(tick, string)` cell on
/// the active track's tab staff — exactly the cell the next typed fret will
/// land in. Mirrors TuxGuitar's "edit caret" model: moving right advances by
/// the currently selected note duration, not by a fixed pixel grid.
#[derive(Clone, Copy, Debug)]
pub struct EditCursor {
    pub tick: Tick,
    pub string_index: usize,
    /// Multi-digit fret being typed (e.g. user presses `1` then `2` -> fret 12).
    /// Cleared on commit, cursor advance, or string change.
    pub pending_fret: Option<u8>,
    /// Instant the pending fret buffer was last touched, so a stale half-typed
    /// number clears itself after a short timeout instead of leaking into the
    /// next edit.
    pub pending_at: Option<std::time::Instant>,
}

impl EditCursor {
    pub fn new(tick: Tick, string_index: usize) -> Self {
        Self {
            tick,
            string_index,
            pending_fret: None,
            pending_at: None,
        }
    }
}

pub struct BetterWriterApp {
    project: BwxProject,
    theme: EditorTheme,
    shadow_timeline: ShadowTimeline,
    audio: AudioRuntime,
    vst_slot: Vst3HostSlot,
    current_file: Option<PathBuf>,
    selected_track_id: u64,
    selected_note_id: Option<u64>,
    selected_duration: DurationChoice,
    active_effects: NoteEffects,
    active_velocity: u8,
    fret_to_insert: u8,
    pixels_per_tick: f32,
    status: String,
    compatibility: CompatibilityReport,
    cursor: EditCursor,
}

impl BetterWriterApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let project = BwxProject::demo();
        let shadow_timeline = ShadowTimeline::default();
        shadow_timeline.rebuild_all(&project);
        let compatibility = CompatibilityReport::analyze(&project, horizon_tick(&project));
        let first_string = project
            .tracks
            .first()
            .map(|track| track.tuning.len().saturating_sub(1))
            .unwrap_or(0);

        Self {
            selected_track_id: project.tracks.first().map(|track| track.id).unwrap_or(1),
            project,
            theme: EditorTheme::default(),
            shadow_timeline,
            audio: AudioRuntime::new(default_soundfont_path()),
            vst_slot: Vst3HostSlot::default(),
            current_file: None,
            selected_note_id: None,
            selected_duration: DurationChoice::Quarter,
            active_effects: NoteEffects::default(),
            active_velocity: 108,
            fret_to_insert: 3,
            pixels_per_tick: 0.07,
            status: "Type 0-9 to enter a fret, arrows to move, Enter to advance. \
                See Help > Keyboard for all shortcuts."
                .to_owned(),
            compatibility,
            cursor: EditCursor::new(0, first_string),
        }
    }

    fn refresh_after_edit(&mut self, track_id: u64) {
        self.shadow_timeline.replace_track(&self.project, track_id);
        self.compatibility =
            CompatibilityReport::analyze(&self.project, horizon_tick(&self.project));
    }

    fn play(&mut self) {
        let events = self.shadow_timeline.snapshot();
        match self.audio.play(&self.project, events) {
            Ok(()) => self.status = "Playback started from the shadow timeline.".to_owned(),
            Err(err) => self.status = format!("Audio unavailable: {err}"),
        }
    }

    fn stop(&mut self) {
        match self.audio.stop() {
            Ok(()) => self.status = "Playback stopped.".to_owned(),
            Err(err) => self.status = format!("Stop command failed: {err}"),
        }
    }

    fn new_project(&mut self) {
        self.project = BwxProject::demo();
        self.current_file = None;
        self.selected_track_id = self
            .project
            .tracks
            .first()
            .map(|track| track.id)
            .unwrap_or(1);
        self.selected_note_id = None;
        self.reset_cursor_for_track(self.selected_track_id);
        self.shadow_timeline.rebuild_all(&self.project);
        self.compatibility =
            CompatibilityReport::analyze(&self.project, horizon_tick(&self.project));
        self.status = "Created a new BetterWriter project.".to_owned();
    }

    fn open_from_explorer(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("BetterWriter", &["bwx"])
            .add_filter("TuxGuitar", &["tg"])
            .add_filter("Guitar Pro", &["gp", "gp3", "gp4", "gp5", "gpx"])
            .pick_file()
        else {
            return;
        };

        match format::load_project(&path) {
            Ok(project) => {
                self.project = project;
                self.current_file = Some(path.clone());
                self.selected_track_id = self
                    .project
                    .tracks
                    .first()
                    .map(|track| track.id)
                    .unwrap_or(1);
                self.selected_note_id = None;
                self.reset_cursor_for_track(self.selected_track_id);
                self.shadow_timeline.rebuild_all(&self.project);
                self.compatibility =
                    CompatibilityReport::analyze(&self.project, horizon_tick(&self.project));
                self.status = format!("Loaded {}", path.display());
            }
            Err(err) => self.status = format!("Open failed: {err}"),
        }
    }

    fn save(&mut self) {
        if let Some(path) = self.current_file.clone() {
            self.save_to_path(&path);
        } else {
            self.save_as();
        }
    }

    fn save_as(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("BetterWriter", &["bwx"])
            .add_filter("TuxGuitar", &["tg"])
            .set_file_name(default_save_name(&self.project))
            .save_file()
        else {
            return;
        };
        self.save_to_path(&path);
    }

    fn save_to_path(&mut self, path: &Path) {
        match format::save_project(path, &self.project) {
            Ok(()) => {
                self.current_file = Some(path.to_path_buf());
                self.status = format!("Saved {}", path.display());
            }
            Err(err) => self.status = format!("Save failed: {err}"),
        }
    }

    fn load_vst3(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("VST3", &["vst3"])
            .pick_file()
        else {
            return;
        };
        match self.vst_slot.load_plugin(&path) {
            Ok(()) => self.status = format!("Loaded VST3 host slot {}", path.display()),
            Err(err) => self.status = format!("VST3 load failed: {err}"),
        }
    }

    fn export_bundle(&mut self, format_kind: StandardExportFormat) {
        let path = PathBuf::from(r"A:\a\1\BetterWriter\compatibility_export.zip");
        let selected: Vec<u64> = self.project.tracks.iter().map(|track| track.id).collect();
        match format::write_multitrack_bundle(
            &path,
            &self.project,
            &selected,
            format_kind,
            &self.compatibility,
        ) {
            Ok(()) => self.status = format!("Wrote {}", path.display()),
            Err(err) => self.status = err.to_string(),
        }
    }

    fn selected_note_mut(&mut self) -> Option<(u64, u64)> {
        let note_id = self.selected_note_id?;
        Some((self.selected_track_id, note_id))
    }

    /// Drop the cursor onto the lowest string of a freshly selected track.
    fn reset_cursor_for_track(&mut self, track_id: u64) {
        self.cursor.tick = 0;
        self.cursor.string_index = self
            .project
            .track(track_id)
            .map(|track| track.tuning.len().saturating_sub(1))
            .unwrap_or(0);
        self.cursor.pending_fret = None;
        self.cursor.pending_at = None;
    }

    /// If the pending fret buffer has gone stale (~1.2s without input), drop it
    /// so a stray `1` doesn't quietly prepend to the next number.
    fn expire_pending_fret(&mut self) {
        let stale = self
            .cursor
            .pending_at
            .is_some_and(|at| at.elapsed() > std::time::Duration::from_millis(1200));
        if stale {
            self.cursor.pending_fret = None;
            self.cursor.pending_at = None;
        }
    }

    /// Advance the cursor by the currently selected note duration. If the
    /// cursor would cross into a new measure whose time signature differs,
    /// `measure_bounds_at` keeps it inside the same bar so writing stays sane.
    fn advance_cursor(&mut self) {
        let step = self.selected_duration.ticks();
        let Some(track) = self.project.track(self.selected_track_id) else {
            self.cursor.tick += step;
            return;
        };
        let (measure_start, measure_end) = track.measure_bounds_at(self.cursor.tick);
        let next = self.cursor.tick + step;
        // Wrap to the next measure rather than drifting past the bar line.
        self.cursor.tick = if next >= measure_end {
            measure_end
        } else {
            next.max(measure_start)
        };
    }

    fn move_cursor_string(&mut self, delta: i32) {
        let Some(track) = self.project.track(self.selected_track_id) else {
            return;
        };
        let count = track.tuning.len() as i32;
        if count == 0 {
            return;
        }
        let raw = self.cursor.string_index as i32 + delta;
        self.cursor.string_index = raw.clamp(0, count - 1) as usize;
        // Changing strings abandons any half-typed fret.
        self.cursor.pending_fret = None;
    }

    /// Push a typed digit into the pending fret buffer. A second digit inside
    /// ~1.2s of the first is treated as the ones column (so `1` then `2` = 12).
    fn push_fret_digit(&mut self, digit: u8) {
        self.expire_pending_fret();
        let Some(track) = self.project.track(self.selected_track_id) else {
            return;
        };
        let max_fret = track.fret_count;
        let candidate = match self.cursor.pending_fret {
            Some(existing) => existing * 10 + digit,
            None => digit,
        };
        let now = std::time::Instant::now();
        if candidate <= max_fret {
            self.cursor.pending_fret = Some(candidate);
            self.cursor.pending_at = Some(now);
            self.status = format!(
                "Fret buffer: {}  (Enter to place, Esc to clear)",
                candidate
            );
        } else {
            // Too large for this instrument: start fresh from the new digit.
            self.cursor.pending_fret = Some(digit);
            self.cursor.pending_at = Some(now);
            self.status = format!(
                "Fret {candidate} exceeds {} frets; starting fresh with {digit}.",
                max_fret
            );
        }
    }

    /// Commit the pending fret (or the toolbar default) as a note at the cursor.
    fn commit_fret_at_cursor(&mut self) {
        let track_id = self.selected_track_id;
        let Some(track) = self.project.track(track_id) else {
            return;
        };
        let string_index = self
            .cursor
            .string_index
            .min(track.tuning.len().saturating_sub(1));
        let tick = self.cursor.tick.max(0);
        let fret = self.cursor.pending_fret.unwrap_or(self.fret_to_insert);
        let duration = self.selected_duration.ticks();

        // If a note already sits on this string at this tick, overwrite its
        // fret instead of stacking a duplicate.
        let existing = self.project.track(track_id).and_then(|t| {
            t.notes
                .iter()
                .find(|n| n.abs_tick == tick && n.string_index == string_index)
                .map(|n| n.id)
        });

        if let Some(id) = existing {
            if let Some(track) = self.project.track_mut(track_id)
                && let Some(note) = track.notes.iter_mut().find(|n| n.id == id)
            {
                note.fret = fret;
                note.velocity = self.active_velocity;
                note.effects = self.active_effects.clone();
                self.selected_note_id = Some(id);
                self.status = format!("Replaced note {id} with fret {fret}.");
            }
        } else if let Some(id) = self.project.insert_note(
            track_id,
            tick,
            duration,
            string_index,
            fret,
        ) {
            if let Some(track) = self.project.track_mut(track_id)
                && let Some(note) = track.notes.iter_mut().find(|n| n.id == id)
            {
                note.velocity = self.active_velocity;
                note.effects = self.active_effects.clone();
            }
            self.selected_note_id = Some(id);
            self.status = format!(
                "Wrote fret {fret} at tick {tick} on string {}.",
                string_index + 1
            );
        }

        self.cursor.pending_fret = None;
        self.refresh_after_edit(track_id);
        self.advance_cursor();
    }

    /// Delete the note sitting under the cursor (if any) and pull the caret
    /// back to that tick so the next entry fills the gap.
    fn delete_at_cursor(&mut self) {
        let track_id = self.selected_track_id;
        let Some(track) = self.project.track(track_id) else {
            return;
        };
        let tick = self.cursor.tick;
        let string_index = self.cursor.string_index;
        let target = track
            .notes
            .iter()
            .find(|n| n.abs_tick == tick && n.string_index == string_index)
            .map(|n| n.id);
        if let Some(id) = target
            && self.project.delete_note(track_id, id)
        {
            self.selected_note_id = None;
            self.refresh_after_edit(track_id);
            self.status = format!("Deleted note {id} at tick {tick}.");
        }
    }

    fn cycle_duration(&mut self, steps: i32) {
        let idx = DurationChoice::ALL
            .iter()
            .position(|d| *d == self.selected_duration)
            .unwrap_or(2) as i32;
        let len = DurationChoice::ALL.len() as i32;
        let next = ((idx + steps).rem_euclid(len)) as usize;
        self.selected_duration = DurationChoice::ALL[next];
        self.status = format!("Note duration: {}", self.selected_duration.label());
    }

    /// Central keyboard controller. Drives the entire write workflow so the
    /// mouse is optional. Returns `true` if any key was consumed so the caller
    /// can request a repaint when the score changes.
    ///
    /// Bindings (mirrors TuxGuitar's tab-entry feel):
    ///   - Digits 0-9: build the fret number (multi-digit allowed)
    ///   - Enter / Space: place the buffered (or default) fret at the cursor
    ///   - Backspace / Delete: remove the note under the cursor
    ///   - Left / Right: move by the selected duration (or one grid cell)
    ///   - Up / Down: change the active string
    ///   - + / = / -: cycle note duration shorter / longer
    ///   - 1-6 (with Ctrl held): jump to a specific duration slot
    ///   - Esc: clear the pending fret buffer
    ///   - Home: snap cursor to tick 0
    fn handle_keyboard(&mut self, ctx: &egui::Context) -> bool {
        self.expire_pending_fret();
        let mut changed = false;

        ctx.input(|i| {
            // Fret digits — always available so the user can start typing
            // immediately. We read raw events to also catch numpad keys.
            // Skip when a modifier is held so Ctrl+digit can drive duration.
            if !i.modifiers.ctrl && !i.modifiers.alt && !i.modifiers.mac_cmd {
                for event in &i.events {
                    if let egui::Event::Text(text) = event
                        && let Some(ch) = text.chars().next()
                        && let Some(digit) = ch.to_digit(10)
                    {
                        self.push_fret_digit(digit as u8);
                        changed = true;
                    }
                }
            }

            // Arrow navigation
            if i.key_pressed(egui::Key::ArrowRight) {
                self.advance_cursor();
                self.selected_note_id = None;
                changed = true;
            }
            if i.key_pressed(egui::Key::ArrowLeft) {
                let step = self.selected_duration.ticks();
                self.cursor.tick = (self.cursor.tick - step).max(0);
                self.cursor.pending_fret = None;
                self.cursor.pending_at = None;
                self.selected_note_id = None;
                changed = true;
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                self.move_cursor_string(1);
                changed = true;
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                self.move_cursor_string(-1);
                changed = true;
            }

            // Commit / delete
            if i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space) {
                self.commit_fret_at_cursor();
                changed = true;
            }
            if i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete) {
                if self.cursor.pending_fret.is_some() {
                    self.cursor.pending_fret = None;
                    self.cursor.pending_at = None;
                    self.status = "Cleared pending fret.".to_owned();
                } else {
                    self.delete_at_cursor();
                }
                changed = true;
            }

            // Duration cycling: `-` → longer, `+`/`=` → shorter
            if i.key_pressed(egui::Key::Minus) {
                self.cycle_duration(1);
                changed = true;
            }
            if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                self.cycle_duration(-1);
                changed = true;
            }

            // Ctrl+digit jumps to a duration slot
            for (slot, key) in [
                (0, egui::Key::Num1),
                (1, egui::Key::Num2),
                (2, egui::Key::Num3),
                (3, egui::Key::Num4),
                (4, egui::Key::Num5),
                (5, egui::Key::Num6),
            ] {
                if i.modifiers.ctrl
                    && i.key_pressed(key)
                    && let Some(&d) = DurationChoice::ALL.get(slot)
                {
                    self.selected_duration = d;
                    self.status = format!("Note duration: {}", d.label());
                    changed = true;
                }
            }

            // Clear pending fret
            if i.key_pressed(egui::Key::Escape) {
                self.cursor.pending_fret = None;
                self.cursor.pending_at = None;
                self.selected_note_id = None;
                self.status = "Selection cleared.".to_owned();
                changed = true;
            }
            if i.key_pressed(egui::Key::Home) {
                self.cursor.tick = 0;
                self.cursor.pending_fret = None;
                self.cursor.pending_at = None;
                changed = true;
            }
        });

        if changed {
            ctx.request_repaint();
        }
        changed
    }
}

impl eframe::App for BetterWriterApp {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Pump keyboard input before any panel owns focus, so the canvas acts
        // like a DAW piano-roll: keys write notes even while a menu is closed.
        self.handle_keyboard(root.ctx());

        egui::Panel::top("menu")
            .exact_size(26.0)
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(238, 238, 238)))
            .show_inside(root, |ui| self.show_menu(ui));

        egui::Panel::top("toolbar")
            .exact_size(42.0)
            .frame(egui::Frame::NONE.fill(self.theme.panel_background))
            .show_inside(root, |ui| self.show_transport_toolbar(ui));

        egui::Panel::left("palette")
            .resizable(false)
            .exact_size(164.0)
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(235, 235, 235)))
            .show_inside(root, |ui| self.show_tux_palette(ui));

        egui::Panel::bottom("fretboard")
            .exact_size(112.0)
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(10, 10, 10)))
            .show_inside(root, |ui| self.show_fretboard(ui));

        egui::Panel::bottom("track_table")
            .exact_size(78.0)
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(239, 239, 239)))
            .show_inside(root, |ui| self.show_track_table(ui));

        egui::Panel::bottom("status")
            .exact_size(28.0)
            .frame(egui::Frame::NONE.fill(self.theme.panel_background))
            .show_inside(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(&self.status);
                    ui.separator();
                    ui.label(format!(
                        "{} shadow MIDI events",
                        self.shadow_timeline.snapshot().len()
                    ));
                    ui.separator();
                    ui.label(self.audio.status());
                    ui.separator();
                    let mut vst_probe = [0.0_f32; 2];
                    self.vst_slot.process_replacing(&[0.0, 0.0], &mut vst_probe);
                    ui.label(self.vst_slot.status());
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.theme.sheet_background))
            .show_inside(root, |ui| self.show_editor(ui));
    }
}

impl BetterWriterApp {
    fn show_menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.new_project();
                    ui.close();
                }
                if ui.button("Open...").clicked() {
                    self.open_from_explorer();
                    ui.close();
                }
                if ui.button("Save").clicked() {
                    self.save();
                    ui.close();
                }
                if ui.button("Save As...").clicked() {
                    self.save_as();
                    ui.close();
                }
                ui.separator();
                let compatible = self.compatibility.standard_exports_enabled;
                let hint = self
                    .compatibility
                    .warning
                    .clone()
                    .unwrap_or_else(|| "Export each track as a standard tab bundle.".to_owned());
                ui.add_enabled_ui(compatible, |ui| {
                    if ui.button("Export .tg Bundle").clicked() {
                        self.export_bundle(StandardExportFormat::TuxGuitar);
                        ui.close();
                    }
                    if ui.button("Export .gp Bundle").clicked() {
                        self.export_bundle(StandardExportFormat::GuitarPro);
                        ui.close();
                    }
                })
                .response
                .on_disabled_hover_text(&hint);
                if !compatible {
                    ui.label(
                        egui::RichText::new(&hint)
                            .small()
                            .color(egui::Color32::from_rgb(170, 90, 60)),
                    );
                }
            });
            for name in [
                "Edit",
                "View",
                "Composition",
                "Track",
                "Measure",
                "Beat",
                "Marker",
                "Player",
                "Tools",
            ] {
                ui.menu_button(name, |ui| {
                    ui.label("BetterWriter native tools are being mapped here.");
                });
            }
            ui.menu_button("Help", |ui| {
                ui.label(egui::RichText::new("Keyboard — Tab Writing").strong());
                ui.separator();
                ui.label("0-9 ............ type fret (multi-digit, e.g. 1 then 2 = 12)");
                ui.label("Enter / Space ... place the buffered fret at the cursor");
                ui.label("Backspace / Del . delete note under cursor (or clear buffer)");
                ui.label("Left / Right .... move cursor by the selected duration");
                ui.label("Up / Down ....... change active string");
                ui.label("+ / = / - ......... cycle note duration shorter / longer");
                ui.label("Ctrl+1..6 ....... jump to duration slot (1/1, 1/2, ... 1/32)");
                ui.label("Esc ............. clear pending fret / selection");
                ui.label("Home ............ move cursor to tick 0");
                ui.separator();
                ui.label(
                    "Tip: click the staff to reposition the cursor, then type a fret.",
                );
            });
        });
    }

    fn show_transport_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            if small_tool_button(ui, "New", "New file").clicked() {
                self.new_project();
            }
            if small_tool_button(ui, "Open", "Open .bwx or .tg").clicked() {
                self.open_from_explorer();
            }
            if small_tool_button(ui, "Save", "Save current project").clicked() {
                self.save();
            }
            if small_tool_button(ui, "Save As", "Save using Explorer").clicked() {
                self.save_as();
            }
            ui.separator();
            if small_tool_button(ui, "|<", "Go to start").clicked() {
                self.status = "Playhead moved to the beginning.".to_owned();
            }
            if small_tool_button(ui, "<<", "Rewind").clicked() {
                self.status = "Rewind requested.".to_owned();
            }
            if small_tool_button(ui, ">", "Play").clicked() {
                self.play();
            }
            if small_tool_button(ui, "[]", "Stop").clicked() {
                self.stop();
            }
            if small_tool_button(ui, ">>", "Fast forward").clicked() {
                self.status = "Fast-forward requested.".to_owned();
            }
            if small_tool_button(ui, ">|", "Go to end").clicked() {
                self.status = "Playhead moved to the end.".to_owned();
            }
            ui.separator();
            ui.label("tempo");
            ui.add(egui::DragValue::new(&mut self.project.tempo_bpm).range(20.0..=320.0));
            ui.label("duration");
            egui::ScrollArea::horizontal()
                .id_salt("duration_scroll")
                .show(ui, |ui| {
                    egui::ComboBox::from_id_salt("duration")
                        .selected_text(self.selected_duration.label())
                        .show_ui(ui, |ui| {
                            for duration in DurationChoice::ALL {
                                ui.selectable_value(
                                    &mut self.selected_duration,
                                    duration,
                                    duration.label(),
                                );
                            }
                        });
                });
            ui.add(
                egui::DragValue::new(&mut self.fret_to_insert)
                    .range(0..=36)
                    .prefix("default fret "),
            )
            .on_hover_text("Fret used when you press Enter without typing a number first");
            ui.add(egui::Slider::new(&mut self.pixels_per_tick, 0.035..=0.14).text("scale"));
            ui.separator();
            if small_tool_button(ui, "VST3", "Load VST3 plug-in").clicked() {
                self.load_vst3();
            }
            if let Some((track_id, note_id)) = self.selected_note_mut() {
                if small_tool_button(ui, "Dur", "Apply selected duration to note").clicked() {
                    self.apply_selected_duration();
                }
                if small_tool_button(ui, "Del", "Delete selected note").clicked()
                    && self.project.delete_note(track_id, note_id)
                {
                    self.selected_note_id = None;
                    self.refresh_after_edit(track_id);
                }
            }
        });
    }

    fn show_tux_palette(&mut self, ui: &mut egui::Ui) {
        palette_section(ui, "Edit", |ui| {
            ui.horizontal_wrapped(|ui| {
                small_tool_button(ui, "Sel", "Select tool");
                small_tool_button(ui, "Draw", "Tab entry tool");
                small_tool_button(ui, "1", "Voice 1");
                small_tool_button(ui, "2", "Voice 2");
            });
        });
        palette_section(ui, "Composition", |ui| {
            ui.horizontal_wrapped(|ui| {
                small_tool_button(ui, "TS", "Time signature");
                small_tool_button(ui, "Key", "Key signature");
                small_tool_button(ui, "Bar+", "Insert measure");
                small_tool_button(ui, "Rpt", "Repeat marker");
            });
        });
        palette_section(ui, "Duration", |ui| {
            ui.horizontal_wrapped(|ui| {
                for duration in DurationChoice::ALL {
                    if ui
                        .selectable_label(self.selected_duration == duration, duration.label())
                        .on_hover_text("Set inserted/selected note duration")
                        .clicked()
                    {
                        self.selected_duration = duration;
                        self.apply_selected_duration();
                    }
                }
            });
        });
        palette_section(ui, "Dynamic", |ui| {
            for (label, velocity) in [
                ("ppp", 30),
                ("pp", 42),
                ("p", 56),
                ("mp", 72),
                ("mf", 88),
                ("f", 104),
                ("ff", 116),
                ("fff", 127),
            ] {
                if ui
                    .selectable_label(self.active_velocity == velocity, label)
                    .clicked()
                {
                    self.active_velocity = velocity;
                    self.apply_velocity_to_selected();
                }
            }
        });
        palette_section(ui, "Effects", |ui| {
            let mut changed = false;
            changed |= ui
                .checkbox(&mut self.active_effects.palm_mute, "P.M.")
                .changed();
            changed |= ui
                .checkbox(&mut self.active_effects.let_ring, "L.R.")
                .changed();
            changed |= ui
                .checkbox(&mut self.active_effects.vibrato, "vib")
                .changed();
            changed |= ui
                .checkbox(&mut self.active_effects.slide, "slide")
                .changed();
            changed |= ui
                .checkbox(&mut self.active_effects.hammer, "H/P")
                .changed();
            changed |= ui
                .checkbox(&mut self.active_effects.staccato, "stac")
                .changed();
            changed |= ui.checkbox(&mut self.active_effects.dead, "dead").changed();
            if changed {
                self.apply_effects_to_selected();
            }
        });
        palette_section(ui, "Beat", |ui| {
            ui.horizontal_wrapped(|ui| {
                small_tool_button(ui, "Text", "Beat text");
                small_tool_button(ui, "Up", "Stem up");
                small_tool_button(ui, "Down", "Stem down");
                small_tool_button(ui, "Tie", "Tie note");
            });
        });
    }

    fn show_track_table(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                if let Some(track) = self.project.track(self.selected_track_id) {
                    ui.label(format!("{} {}", track.name, track.program));
                    ui.add(egui::Slider::new(&mut self.active_velocity, 1..=127).text("velocity"));
                }
            });
            ui.separator();
            egui::Grid::new("track_table_grid")
                .striped(true)
                .min_col_width(72.0)
                .show(ui, |ui| {
                    ui.strong("N");
                    ui.strong("S-M");
                    ui.strong("Name");
                    ui.strong("Instrument");
                    ui.strong("Ch");
                    ui.end_row();
                    let mut selected_after = None;
                    for track in &self.project.tracks {
                        let selected = track.id == self.selected_track_id;
                        if ui
                            .selectable_label(selected, track.id.to_string())
                            .clicked()
                        {
                            selected_after = Some(track.id);
                        }
                        ui.label(format!(
                            "{} {}",
                            if track.solo { "S" } else { "-" },
                            if track.mute { "M" } else { "-" }
                        ));
                        if ui.selectable_label(selected, &track.name).clicked() {
                            selected_after = Some(track.id);
                        }
                        ui.label(gm_program_name(track.program));
                        ui.label((track.channel + 1).to_string());
                        ui.end_row();
                    }
                    if let Some(track_id) = selected_after {
                        self.selected_track_id = track_id;
                        self.selected_note_id = None;
                        self.reset_cursor_for_track(track_id);
                    }
                });
        });
    }

    fn show_fretboard(&mut self, ui: &mut egui::Ui) {
        let desired = ui.available_size();
        let (rect, _response) = ui.allocate_exact_size(desired, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::BLACK);
        let Some(track) = self.project.track(self.selected_track_id) else {
            return;
        };
        let left = rect.left() + 8.0;
        let right = rect.right() - 8.0;
        let top = rect.top() + 10.0;
        let bottom = rect.bottom() - 10.0;
        let string_gap = (bottom - top) / track.tuning.len().max(1) as f32;
        for string in 0..track.tuning.len() {
            let y = top + string as f32 * string_gap;
            painter.line_segment(
                [Pos2::new(left, y), Pos2::new(right, y)],
                Stroke::new(1.0, egui::Color32::from_gray(155)),
            );
            painter.text(
                Pos2::new(left - 4.0, y),
                Align2::RIGHT_CENTER,
                string_label(track.tuning[string]),
                FontId::monospace(10.0),
                egui::Color32::from_gray(210),
            );
        }
        let frets = 20;
        for fret in 0..=frets {
            let x = left + (right - left) * fret as f32 / frets as f32;
            painter.line_segment(
                [Pos2::new(x, top - 5.0), Pos2::new(x, bottom + 5.0)],
                Stroke::new(
                    if fret == 0 { 3.0 } else { 1.0 },
                    egui::Color32::from_gray(180),
                ),
            );
        }
        for fret in [3, 5, 7, 9, 12, 15, 17] {
            let x = left + (right - left) * (fret as f32 - 0.5) / frets as f32;
            painter.circle_filled(
                Pos2::new(x, (top + bottom) * 0.5),
                if fret == 12 { 4.0 } else { 3.0 },
                egui::Color32::from_gray(200),
            );
        }
    }

    fn apply_selected_duration(&mut self) {
        if let Some((track_id, note_id)) = self.selected_note_mut() {
            if self.project.set_note_duration_fluid(
                track_id,
                note_id,
                self.selected_duration.ticks(),
            ) {
                self.refresh_after_edit(track_id);
                self.status = "Duration changed; later notes in the measure shifted.".to_owned();
            }
        }
    }

    fn apply_velocity_to_selected(&mut self) {
        let Some(note_id) = self.selected_note_id else {
            return;
        };
        if let Some(track) = self.project.track_mut(self.selected_track_id)
            && let Some(note) = track.notes.iter_mut().find(|note| note.id == note_id)
        {
            note.velocity = self.active_velocity;
            self.refresh_after_edit(self.selected_track_id);
        }
    }

    fn apply_effects_to_selected(&mut self) {
        let Some(note_id) = self.selected_note_id else {
            return;
        };
        if let Some(track) = self.project.track_mut(self.selected_track_id)
            && let Some(note) = track.notes.iter_mut().find(|note| note.id == note_id)
        {
            note.effects = self.active_effects.clone();
            self.refresh_after_edit(self.selected_track_id);
        }
    }

    fn show_editor(&mut self, ui: &mut egui::Ui) {
        let desired = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(247, 247, 247));

        let page = rect.shrink2(Vec2::new(18.0, 18.0));
        painter.rect_filled(page, 0.0, self.theme.sheet_background);

        let track_rect = self.paint_selected_track_page(&painter, page);
        self.paint_shadow_overview(&painter, page);

        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.handle_canvas_click(pos, track_rect);
        }
    }

    fn paint_selected_track_page(&self, painter: &egui::Painter, page: Rect) -> Rect {
        let Some(track) = self.project.track(self.selected_track_id) else {
            return page;
        };
        let left = page.left() + 54.0;
        let right = page.right() - 18.0;
        let width_ticks = ((right - left) / self.pixels_per_tick).max(TICKS_PER_QUARTER as f32);
        let system_ticks = snap_tick(width_ticks as Tick, TICKS_PER_QUARTER).max(TICKS_PER_QUARTER);
        let system_height = 168.0;
        let first_top = page.top() + 56.0;
        let systems = ((horizon_tick(&self.project) as f32 / system_ticks as f32).ceil() as usize)
            .max(1)
            .min(8);

        painter.text(
            Pos2::new(page.left() + 16.0, page.top() + 12.0),
            Align2::LEFT_TOP,
            format!("{} - {}", self.project.title, track.name),
            FontId::proportional(15.0),
            self.theme.notation_foreground,
        );

        for system in 0..systems {
            let system_origin_tick = system as Tick * system_ticks;
            let top = first_top + system as f32 * system_height;
            if top + 136.0 > page.bottom() {
                break;
            }
            let staff_y = top;
            let tab_y = top + 82.0;
            for line in 0..5 {
                let y = staff_y + line as f32 * 7.0;
                painter.line_segment(
                    [Pos2::new(left, y), Pos2::new(right, y)],
                    self.theme.staff_stroke(),
                );
            }
            for string in 0..track.tuning.len() {
                let y = tab_y + string as f32 * 9.0;
                painter.line_segment(
                    [Pos2::new(left, y), Pos2::new(right, y)],
                    self.theme.staff_stroke(),
                );
            }
            painter.text(
                Pos2::new(left - 26.0, staff_y + 14.0),
                Align2::CENTER_CENTER,
                "G",
                FontId::proportional(28.0),
                egui::Color32::BLACK,
            );

            let mut bar_tick = system_origin_tick;
            while bar_tick <= system_origin_tick + system_ticks {
                let x = left + (bar_tick - system_origin_tick) as f32 * self.pixels_per_tick;
                if x > right {
                    break;
                }
                painter.line_segment(
                    [Pos2::new(x, staff_y), Pos2::new(x, tab_y + 48.0)],
                    self.theme.bar_stroke(),
                );
                let sig = track.signature_at(bar_tick);
                painter.text(
                    Pos2::new(x + 6.0, staff_y + 14.0),
                    Align2::LEFT_CENTER,
                    format!("{}\n{}", sig.numerator, sig.denominator),
                    FontId::monospace(14.0),
                    egui::Color32::BLACK,
                );
                bar_tick += sig.measure_ticks().max(TICKS_PER_QUARTER / 4);
            }

            self.paint_cursor_cell(painter, left, tab_y, system_origin_tick, system_ticks);

            for note in track.notes.iter().filter(|note| {
                note.abs_tick >= system_origin_tick
                    && note.abs_tick < system_origin_tick + system_ticks
            }) {
                let x = left + (note.abs_tick - system_origin_tick) as f32 * self.pixels_per_tick;
                let tab_line_y = tab_y + note.string_index as f32 * 9.0;
                let staff_note_y = staff_y + 28.0 - (note.fret as f32 % 12.0) * 1.7;
                let selected = self.selected_note_id == Some(note.id);
                painter.circle_filled(
                    Pos2::new(x + 8.0, staff_note_y),
                    4.2,
                    if selected {
                        self.theme.accent_color
                    } else {
                        egui::Color32::from_rgb(58, 58, 58)
                    },
                );
                painter.line_segment(
                    [
                        Pos2::new(x + 13.0, staff_note_y),
                        Pos2::new(x + 13.0, staff_note_y - 24.0),
                    ],
                    Stroke::new(1.4, egui::Color32::from_rgb(45, 45, 45)),
                );
                // Note brick width is strictly proportional to the note's
                // duration in ticks, so a 1/32 reads narrower than a 1/16.
                // We only floor it wide enough to keep the fret digit legible
                // (two-digit frets need a touch more room than one-digit ones).
                let proportional = note.duration_ticks as f32 * self.pixels_per_tick;
                let min_for_digits = if note.fret >= 10 { 13.0 } else { 8.0 };
                let width = proportional.max(min_for_digits);
                // Slightly shorter bricks for very brief notes so the visual
                // rhythm matches the musical one, without squashing legible ones.
                let height = if proportional < 10.0 { 9.0 } else { 12.0 };
                let note_rect = Rect::from_center_size(
                    Pos2::new(x + width * 0.5, tab_line_y),
                    Vec2::new(width, height),
                );
                painter.rect(
                    note_rect,
                    1.0,
                    if selected {
                        self.theme.accent_color
                    } else {
                        self.theme.note_fill
                    },
                    self.theme.note_stroke(),
                    StrokeKind::Middle,
                );
                // Shrink the font for cramped bricks so the number still fits.
                let font_size = if width < 11.0 { 7.0 } else { 9.0 };
                painter.text(
                    note_rect.center(),
                    Align2::CENTER_CENTER,
                    note.fret.to_string(),
                    FontId::monospace(font_size),
                    if selected {
                        egui::Color32::WHITE
                    } else {
                        self.theme.notation_foreground
                    },
                );
                if note.effects.palm_mute {
                    painter.text(
                        Pos2::new(x + 2.0, tab_y + 58.0),
                        Align2::LEFT_TOP,
                        "P.M.",
                        FontId::monospace(8.0),
                        self.theme.muted_foreground,
                    );
                }
            }
            for rest in self
                .project
                .tail_padding_rests(track.id, system_origin_tick)
                .into_iter()
                .filter(|rest| {
                    rest.abs_tick >= system_origin_tick
                        && rest.abs_tick < system_origin_tick + system_ticks
                })
            {
                let x = left + (rest.abs_tick - system_origin_tick) as f32 * self.pixels_per_tick;
                painter.line_segment(
                    [
                        Pos2::new(x, tab_y + 62.0),
                        Pos2::new(x + 14.0, tab_y + 62.0),
                    ],
                    Stroke::new(2.0, self.theme.rest_color),
                );
            }
        }

        page
    }

    /// Draw the keyboard write-cursor: a soft accent box over the active
    /// `(tick, string)` cell, plus the half-typed fret as ghost text so the
    /// user sees what they're composing before committing.
    fn paint_cursor_cell(
        &self,
        painter: &egui::Painter,
        left: f32,
        tab_y: f32,
        system_origin_tick: Tick,
        system_ticks: Tick,
    ) {
        // Only render if the cursor is inside this system's tick window.
        if self.cursor.tick < system_origin_tick
            || self.cursor.tick >= system_origin_tick + system_ticks
        {
            return;
        }
        let string_gap = 9.0;
        // Mirror the note-brick sizing: the cursor preview should show exactly
        // the width of the brick that will land there, so a 1/32 cursor looks
        // narrower than a 1/16 cursor. Keep a small floor so the caret box
        // stays visible/targetable even at the shortest durations.
        let proportional = self.selected_duration.ticks() as f32 * self.pixels_per_tick;
        let cell_width = proportional.max(7.0);
        let x = left + (self.cursor.tick - system_origin_tick) as f32 * self.pixels_per_tick;
        let y = tab_y + self.cursor.string_index as f32 * string_gap;

        // Translucent accent fill derived from the theme's accent color so the
        // highlight stays readable on light or dark palettes.
        let [ar, ag, ab, _] = self.theme.accent_color.to_array();
        let fill = egui::Color32::from_rgba_unmultiplied(ar, ag, ab, 70);
        let ghost = egui::Color32::from_rgba_unmultiplied(ar, ag, ab, 160);

        // Filled highlight box.
        let cell = Rect::from_center_size(
            Pos2::new(x + cell_width * 0.5, y),
            Vec2::new(cell_width, string_gap + 2.0),
        );
        painter.rect_filled(cell, 2.0, fill);
        painter.rect_stroke(
            cell,
            2.0,
            Stroke::new(1.5, self.theme.accent_color),
            StrokeKind::Middle,
        );

        // Ghost text for the pending fret, or a caret hint when empty.
        let label = match self.cursor.pending_fret {
            Some(fret) => fret.to_string(),
            None => "_".to_owned(),
        };
        painter.text(
            cell.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::monospace(10.0),
            ghost,
        );
    }

    fn paint_shadow_overview(&self, painter: &egui::Painter, rect: Rect) {
        let events: Vec<MidiPlaybackEvent> = self.shadow_timeline.snapshot();
        let y = rect.bottom() - 14.0;
        let left = rect.left() + 54.0;
        painter.line_segment(
            [Pos2::new(left, y), Pos2::new(rect.right() - 24.0, y)],
            Stroke::new(1.0, self.theme.muted_foreground),
        );
        for event in events
            .iter()
            .filter(|event| matches!(event.kind, MidiEventKind::NoteOn))
        {
            let x = left + event.tick as f32 * self.pixels_per_tick;
            if x > rect.right() - 24.0 {
                continue;
            }
            painter.circle_filled(Pos2::new(x, y), 2.5, self.theme.playhead_color);
        }
    }

    fn handle_canvas_click(&mut self, pos: Pos2, page: Rect) {
        if !page.contains(pos) {
            return;
        }
        let track_id = self.selected_track_id;
        let Some(track) = self.project.track(track_id) else {
            return;
        };
        let left = page.left() + 54.0;
        let right = page.right() - 18.0;
        let system_ticks = snap_tick(
            (((right - left) / self.pixels_per_tick).max(TICKS_PER_QUARTER as f32)) as Tick,
            TICKS_PER_QUARTER,
        )
        .max(TICKS_PER_QUARTER);
        let system_height = 168.0;
        let first_top = page.top() + 56.0;
        let system_index = ((pos.y - first_top) / system_height).floor().max(0.0) as Tick;
        let system_origin_tick = system_index * system_ticks;
        let clicked_tick =
            (((pos.x - left) / self.pixels_per_tick).max(0.0)) as Tick + system_origin_tick;
        let snapped_tick = snap_tick(clicked_tick, TICKS_PER_QUARTER / 4);
        let tab_y = first_top + system_index as f32 * system_height + 82.0;
        let string_gap = 9.0;

        if let Some(note_id) = track.notes.iter().find_map(|note| {
            if note.abs_tick < system_origin_tick
                || note.abs_tick >= system_origin_tick + system_ticks
            {
                return None;
            }
            let x = left + (note.abs_tick - system_origin_tick) as f32 * self.pixels_per_tick;
            let width = (note.duration_ticks as f32 * self.pixels_per_tick).max(22.0);
            let y = tab_y + note.string_index as f32 * string_gap;
            let rect =
                Rect::from_center_size(Pos2::new(x + width * 0.5, y), Vec2::new(width, 14.0));
            rect.contains(pos).then_some(note.id)
        }) {
            self.selected_note_id = Some(note_id);
            if let Some(note) = track.notes.iter().find(|note| note.id == note_id) {
                self.active_velocity = note.velocity;
                self.active_effects = note.effects.clone();
                // Park the cursor on the clicked note so subsequent typing edits it.
                self.cursor.tick = note.abs_tick;
                self.cursor.string_index = note.string_index;
                self.cursor.pending_fret = None;
                self.cursor.pending_at = None;
            }
            self.status = format!("Selected note {note_id}.");
            return;
        }

        let string_index = ((pos.y - tab_y + string_gap * 0.5) / string_gap)
            .floor()
            .clamp(0.0, track.tuning.len().saturating_sub(1) as f32)
            as usize;

        // A click on empty space repositions the keyboard cursor so the user
        // can click-then-type. Inserting a note is now done with the keyboard
        // (digits + Enter), keeping mouse and keyboard workflows unified.
        self.cursor.tick = snapped_tick;
        self.cursor.string_index = string_index;
        self.cursor.pending_fret = None;
        self.cursor.pending_at = None;
        self.selected_note_id = None;
        self.status = format!(
            "Cursor at tick {} on string {}. Type a fret and press Enter.",
            snapped_tick,
            string_index + 1
        );
    }
}

fn snap_tick(tick: Tick, grid: Tick) -> Tick {
    if grid <= 0 {
        tick
    } else {
        (tick / grid) * grid
    }
}

fn horizon_tick(project: &BwxProject) -> Tick {
    project
        .tracks
        .iter()
        .flat_map(|track| track.notes.iter().map(|note| note.end_tick()))
        .max()
        .unwrap_or(TICKS_PER_QUARTER * 4)
        + TICKS_PER_QUARTER * 4
}

fn string_label(midi: u8) -> &'static str {
    match midi % 12 {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        _ => "B",
    }
}

fn default_soundfont_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("soundfonts")
        .join("MS Basic.sf3")
}

fn default_save_name(project: &BwxProject) -> String {
    let mut name: String = project
        .title
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() {
        name = "Untitled".to_owned();
    }
    format!("{name}.bwx")
}

fn small_tool_button(ui: &mut egui::Ui, label: &str, tooltip: &str) -> egui::Response {
    ui.add_sized([42.0, 22.0], egui::Button::new(label))
        .on_hover_text(tooltip)
}

fn palette_section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(egui::Color32::from_rgb(246, 246, 246))
        .stroke(Stroke::new(1.0, egui::Color32::from_rgb(196, 196, 196)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(title).small().strong());
            add_contents(ui);
        });
    ui.add_space(4.0);
}

fn gm_program_name(program: u8) -> &'static str {
    match program {
        24 => "Nylon Guitar",
        25 => "Steel Guitar",
        26 => "Jazz Guitar",
        27 => "Clean Guitar",
        28 => "Muted Guitar",
        29 => "Overdrive Guitar",
        30 => "Distortion Guitar",
        31 => "Guitar Harmonics",
        32 => "Acoustic Bass",
        33 => "Finger Bass",
        34 => "Pick Bass",
        _ => "General MIDI",
    }
}
