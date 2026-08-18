use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::{
    audio::{AudioRuntime, Vst3HostSlot, VstHost},
    core::{
        BwxProject, CompatibilityReport, DurationChoice, InstrumentFamily, InstrumentTrack,
        MidiEventKind, MidiPlaybackEvent, NoteEffects, ShadowTimeline, StandardExportFormat,
        StretchEdge, StringedVariant, TICKS_PER_QUARTER, TempoTransition, Tick, TimeSignature,
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

/// One row ("system") of the tab staff: a contiguous run of whole bars laid
/// out left to right. See `BetterWriterApp::layout_staff_systems`.
struct StaffSystem {
    start_tick: Tick,
    end_tick: Tick,
    bars: Vec<(Tick, Tick)>,
}

impl StaffSystem {
    fn from_bars(bars: Vec<(Tick, Tick)>) -> Self {
        let start_tick = bars.first().map(|bar| bar.0).unwrap_or(0);
        let end_tick = bars.last().map(|bar| bar.1).unwrap_or(start_tick);
        Self {
            start_tick,
            end_tick,
            bars,
        }
    }

    fn tick_span(&self) -> Tick {
        (self.end_tick - self.start_tick).max(0)
    }
}

/// Which automation lane is being edited in the Automation Editor. Only
/// `Tempo` is wired up to real project data today; `Volume`/`Pan` are shown
/// (mirroring Guitar Pro's dropdown) but disabled until per-track automation
/// exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutomationType {
    Tempo,
    Volume,
    Pan,
}

impl AutomationType {
    const ALL: [AutomationType; 3] = [
        AutomationType::Tempo,
        AutomationType::Volume,
        AutomationType::Pan,
    ];

    fn label(self) -> &'static str {
        match self {
            AutomationType::Tempo => "Tempo",
            AutomationType::Volume => "Volume",
            AutomationType::Pan => "Pan",
        }
    }

    fn is_implemented(self) -> bool {
        matches!(self, AutomationType::Tempo)
    }
}

/// What the graph-area drag gesture is currently doing.
#[derive(Clone, Debug, PartialEq)]
enum AutomationDragMode {
    Idle,
    /// Dragging one or more selected points together. Elastic snapping (see
    /// `show_tempo_graph`) needs each point's position *as it was when the
    /// drag started* — recomputing every frame from a fixed baseline plus
    /// the total raw drag distance, rather than incrementally re-snapping
    /// an already-snapped position, is what keeps a small nudge from
    /// compounding into a runaway jump.
    MovingPoints {
        /// (tick, bpm) of every selected point at drag start, in the same
        /// order as `AutomationEditorState::selected_points` at that moment.
        original_points: Vec<(Tick, f32)>,
        /// Index into `original_points` of the point actually grabbed —
        /// only *its* raw distance from a grid line decides whether the
        /// whole selection is currently snapped; everything else just
        /// follows the same effective offset.
        anchor_index: usize,
        /// Total raw (unsnapped) drag distance accumulated since the
        /// gesture started.
        raw_offset_ticks: f64,
        raw_offset_bpm: f32,
    },
    /// Rubber-band selecting: drag started on empty canvas.
    AreaSelecting { start_pos: Pos2, current_pos: Pos2 },
}

/// What the staff canvas's drag gesture is currently doing.
#[derive(Clone, Debug, PartialEq)]
enum CanvasDragMode {
    Idle,
    /// Rubber-band selecting note blocks: drag started on empty canvas.
    AreaSelecting { start_pos: Pos2, current_pos: Pos2 },
    /// Dragging the left or right edge of a note to resize it. If several
    /// notes are area-selected and the grabbed note is one of them, every
    /// note in the selection stretches together on the same edge by the
    /// same amount; otherwise just the grabbed note does.
    Stretching {
        note_ids: Vec<u64>,
        edge: StretchEdge,
        start_pos: Pos2,
        current_pos: Pos2,
    },
    /// Dragging the body (not an edge) of an already-selected note to move
    /// it in time. Like `Stretching`, applies to the whole selection if the
    /// grabbed note is part of one.
    MovingNotes {
        note_ids: Vec<u64>,
        /// The specific note grabbed — its bar start anchors the snap grid
        /// (see `finish_canvas_drag`), and its original position is what
        /// the raw drag distance is measured from.
        anchor_note_id: u64,
        start_pos: Pos2,
        current_pos: Pos2,
    },
}

/// UI-only state for the floating Automation Editor window (Guitar-Pro
/// style tempo/volume/pan curve editor). `None` on `BetterWriterApp` means
/// the window is closed.
struct AutomationEditorState {
    automation_type: AutomationType,
    /// Ticks of the currently selected points (tempo automation only has one
    /// timeline today, so this doubles as "selected tempo points"). Multiple
    /// points can be area-selected and moved/deleted together.
    selected_points: Vec<Tick>,
    /// Ticks-per-pixel for the automation graph, independent of the main
    /// staff's zoom.
    pixels_per_tick: f32,
    snap_to_grid: bool,
    /// Timestamps of recent "TAP" button presses, for computing a live bpm
    /// from the user's tapped rhythm (cleared after ~2s of inactivity).
    tap_times: Vec<std::time::Instant>,
    drag_mode: AutomationDragMode,
    /// Project snapshot taken when a drag gesture starts, so the whole drag
    /// collapses into a single undo step instead of one per frame.
    drag_started_snapshot: Option<BwxProject>,
    /// The transition new points are created with, and — while any points
    /// are selected — the transition the Constant/Progressive control
    /// applies to that selection. Persists across selection changes (it's a
    /// "brush" setting, not a reflection of whatever's currently selected).
    default_transition: TempoTransition,
}

impl AutomationEditorState {
    fn new(selected_tick: Option<Tick>) -> Self {
        Self {
            automation_type: AutomationType::Tempo,
            selected_points: selected_tick.into_iter().collect(),
            pixels_per_tick: 0.07,
            snap_to_grid: true,
            tap_times: Vec::new(),
            drag_mode: AutomationDragMode::Idle,
            drag_started_snapshot: None,
            default_transition: TempoTransition::Constant,
        }
    }
}

/// Which top-level screen is currently shown. The New Project dialog is a
/// floating `egui::Window` layered on top of either screen, not a screen of
/// its own (see `new_project_dialog`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppScreen {
    StartMenu,
    Editor,
}

/// The user's theme preference (View menu). Resolved to an actual
/// `EditorTheme` + `egui::Theme` every frame in `resolve_theme`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppThemeMode {
    /// Follow the OS's light/dark preference; falls back to Dark if egui
    /// can't determine it (some platforms/window managers don't report it).
    System,
    Dark,
    Bright,
}

impl AppThemeMode {
    const ALL: [AppThemeMode; 3] = [
        AppThemeMode::System,
        AppThemeMode::Dark,
        AppThemeMode::Bright,
    ];

    fn label(self) -> &'static str {
        match self {
            AppThemeMode::System => "System",
            AppThemeMode::Dark => "Dark",
            AppThemeMode::Bright => "Bright",
        }
    }
}

/// In-progress selections for the New Project dialog.
struct NewProjectDraft {
    name: String,
    family: InstrumentFamily,
    stringed_variant: StringedVariant,
    string_count: u32,
}

impl NewProjectDraft {
    fn new(default_name: String) -> Self {
        Self {
            name: default_name,
            family: InstrumentFamily::Stringed,
            stringed_variant: StringedVariant::AcousticGuitar,
            string_count: StringedVariant::AcousticGuitar.default_string_count() as u32,
        }
    }
}

const MAX_RECENT_PROJECTS: usize = 10;

pub struct BetterWriterApp {
    project: BwxProject,
    theme: EditorTheme,
    theme_mode: AppThemeMode,
    shadow_timeline: ShadowTimeline,
    audio: AudioRuntime,
    vst_slot: Vst3HostSlot,
    current_file: Option<PathBuf>,
    selected_track_id: u64,
    selected_note_id: Option<u64>,
    /// Notes rubber-band-selected together on the staff canvas (distinct
    /// from `selected_note_id`, which is a single click-to-select). Renders
    /// with the same selected color; supports batch delete.
    area_selected_note_ids: Vec<u64>,
    canvas_drag: CanvasDragMode,
    /// Snap increment for the drag-to-stretch note-edge gesture. Defaults to
    /// a 1/16 note; configurable via RMB > Note > Stretching.
    stretch_step_ticks: Tick,
    /// Snap increment for the drag-to-move gesture (relative to the grabbed
    /// note's bar start; see `finish_canvas_drag`). Configurable via RMB >
    /// Note > Dragging.
    drag_step_ticks: Tick,
    selected_duration: DurationChoice,
    active_effects: NoteEffects,
    active_velocity: u8,
    fret_to_insert: u8,
    pixels_per_tick: f32,
    status: String,
    compatibility: CompatibilityReport,
    cursor: EditCursor,
    /// Whole-project snapshots make every editor operation reversible without
    /// coupling the UI to individual edit command types. The score is small
    /// enough at this stage that this is both simple and responsive.
    undo_stack: Vec<BwxProject>,
    redo_stack: Vec<BwxProject>,
    time_signature_numerator: u8,
    time_signature_denominator: u8,
    screen: AppScreen,
    /// Most-recently-opened/saved project paths, newest first. Persisted to
    /// a small text file next to the executable so the Start Menu's recent
    /// list survives across launches.
    recent_projects: Vec<PathBuf>,
    new_project_dialog: Option<NewProjectDraft>,
    automation_editor: Option<AutomationEditorState>,
    /// Live text in the Start Menu's "Search" box; re-sorts (doesn't filter)
    /// the recent-projects list while non-empty.
    start_menu_search: String,
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
            theme_mode: AppThemeMode::System,
            shadow_timeline,
            audio: AudioRuntime::new(default_soundfont_path()),
            vst_slot: Vst3HostSlot::default(),
            current_file: None,
            selected_note_id: None,
            area_selected_note_ids: Vec::new(),
            canvas_drag: CanvasDragMode::Idle,
            stretch_step_ticks: TICKS_PER_QUARTER / 4,
            drag_step_ticks: TICKS_PER_QUARTER / 4,
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
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            time_signature_numerator: 4,
            time_signature_denominator: 4,
            screen: AppScreen::StartMenu,
            recent_projects: load_recent_projects(),
            new_project_dialog: None,
            automation_editor: None,
            start_menu_search: String::new(),
        }
    }

    /// Resolves `theme_mode` into an actual `EditorTheme` and applies it,
    /// including telling egui itself (`ctx.set_theme`) so built-in widgets
    /// (buttons, windows, menus, text fields — everything not painted by
    /// hand on the staff canvas) follow along too. Runs every frame since
    /// it's cheap and the OS preference can change at runtime under
    /// `System`.
    fn resolve_theme(&mut self, ctx: &egui::Context) {
        let dark = match self.theme_mode {
            AppThemeMode::Dark => true,
            AppThemeMode::Bright => false,
            // `None` means egui couldn't determine the OS preference (some
            // platforms/window managers don't report one) — fall back to
            // Dark rather than silently guessing Bright.
            AppThemeMode::System => {
                !matches!(ctx.input(|input| input.raw.system_theme), Some(egui::Theme::Light))
            }
        };
        self.theme = if dark {
            EditorTheme::dark()
        } else {
            EditorTheme::bright()
        };
        ctx.set_theme(if dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        });
    }

    fn refresh_after_edit(&mut self, track_id: u64) {
        self.shadow_timeline.replace_track(&self.project, track_id);
        self.compatibility =
            CompatibilityReport::analyze(&self.project, horizon_tick(&self.project));
    }

    /// Save a previous project state only when an operation actually changed
    /// the score. A new edit always starts a new undo branch, so redo history
    /// is then no longer meaningful.
    fn record_project_edit(&mut self, before: BwxProject) {
        if before == self.project {
            return;
        }

        const HISTORY_LIMIT: usize = 100;
        self.undo_stack.push(before);
        if self.undo_stack.len() > HISTORY_LIMIT {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn restore_history_state(&mut self, project: BwxProject, action: &str) {
        self.project = project;
        if self.project.track(self.selected_track_id).is_none() {
            self.selected_track_id = self
                .project
                .tracks
                .first()
                .map(|track| track.id)
                .unwrap_or(1);
        }
        self.selected_note_id = self.selected_note_id.filter(|note_id| {
            self.project
                .track(self.selected_track_id)
                .is_some_and(|track| track.notes.iter().any(|note| note.id == *note_id))
        });
        if let Some(track) = self.project.track(self.selected_track_id) {
            self.cursor.string_index = self
                .cursor
                .string_index
                .min(track.tuning.len().saturating_sub(1));
        }
        self.cursor.pending_fret = None;
        self.cursor.pending_at = None;
        self.sync_time_signature_picker();
        self.shadow_timeline.rebuild_all(&self.project);
        self.compatibility =
            CompatibilityReport::analyze(&self.project, horizon_tick(&self.project));
        self.status = format!("{action}.");
    }

    fn undo(&mut self) {
        let Some(previous) = self.undo_stack.pop() else {
            self.status = "Nothing to undo.".to_owned();
            return;
        };
        self.redo_stack.push(self.project.clone());
        self.restore_history_state(previous, "Undid last edit");
    }

    fn redo(&mut self) {
        let Some(next) = self.redo_stack.pop() else {
            self.status = "Nothing to redo.".to_owned();
            return;
        };
        self.undo_stack.push(self.project.clone());
        self.restore_history_state(next, "Redid edit");
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

    /// Resets every piece of per-project UI state (selection, cursor, undo
    /// history, shadow timeline, compatibility report) after swapping in a
    /// brand-new `self.project`. Shared by every "load a different project"
    /// path: opening a recent/browsed file, the demo, or a freshly-created
    /// project.
    fn adopt_loaded_project(&mut self, status: String) {
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
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.status = status;
        self.screen = AppScreen::Editor;
    }

    /// Opens the New Project dialog, pre-filled with a default name that
    /// doesn't collide with any recently-used project.
    fn new_project(&mut self) {
        let default_name = unique_project_name("Untitled", &self.recent_projects);
        self.new_project_dialog = Some(NewProjectDraft::new(default_name));
    }

    fn create_new_project_from_dialog(&mut self) {
        let Some(draft) = self.new_project_dialog.take() else {
            return;
        };
        let name = unique_project_name(&draft.name, &self.recent_projects);
        // Only `Stringed` tracks are implemented today; other families still
        // build a usable (if temporary) stringed track underneath.
        let family = if draft.family.is_implemented() {
            draft.family
        } else {
            InstrumentFamily::Stringed
        };
        let string_count = draft.string_count.max(1) as usize;
        self.project =
            BwxProject::empty_with_track(name, family, draft.stringed_variant, string_count);
        self.current_file = None;
        let title = self.project.title.clone();
        self.adopt_loaded_project(format!("Created new project \"{title}\"."));
    }

    /// Loads the built-in demo project — what used to open automatically on
    /// startup, now reachable from the Start Menu's "Demo Project" button.
    fn load_demo_project(&mut self) {
        self.project = BwxProject::demo();
        self.current_file = None;
        self.adopt_loaded_project("Opened the BetterWriter demo project.".to_owned());
    }

    fn open_project_path(&mut self, path: &Path) {
        match format::load_project(path) {
            Ok(project) => {
                self.project = project;
                self.current_file = Some(path.to_path_buf());
                self.remember_recent_project(path);
                self.adopt_loaded_project(format!("Loaded {}", path.display()));
            }
            Err(err) => self.status = format!("Open failed: {err}"),
        }
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
        self.open_project_path(&path);
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
                self.remember_recent_project(path);
            }
            Err(err) => self.status = format!("Save failed: {err}"),
        }
    }

    /// Moves `path` to the front of the recent-projects list (deduplicating)
    /// and persists the list to disk.
    fn remember_recent_project(&mut self, path: &Path) {
        self.recent_projects.retain(|existing| existing != path);
        self.recent_projects.insert(0, path.to_path_buf());
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);
        save_recent_projects(&self.recent_projects);
    }

    fn return_to_start_menu(&mut self) {
        self.recent_projects = load_recent_projects();
        self.screen = AppScreen::StartMenu;
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
        self.sync_time_signature_picker();
    }

    fn sync_time_signature_picker(&mut self) {
        if let Some(track) = self.project.track(self.selected_track_id) {
            let signature = track.signature_at(self.cursor.tick);
            self.time_signature_numerator = signature.numerator;
            self.time_signature_denominator = signature.denominator;
        }
    }

    fn apply_time_signature_at_cursor(&mut self) {
        let signature = TimeSignature::new(
            self.time_signature_numerator.max(1),
            self.time_signature_denominator.max(1),
        );
        // A bar-level edit applies to the bar containing the click, not to a
        // new boundary at the click itself. This lets a user right-click in
        // the middle of a 4/4 bar and turn that entire bar into (say) 3/4.
        let bar_start = self
            .project
            .track(self.selected_track_id)
            .map(|track| track.measure_bounds_at(self.cursor.tick).0)
            .unwrap_or(self.cursor.tick);
        let previous_project = self.project.clone();
        if self
            .project
            .set_track_time_signature(self.selected_track_id, bar_start, signature)
        {
            self.record_project_edit(previous_project);
            self.refresh_after_edit(self.selected_track_id);
            self.status = format!(
                "Set this bar to {}/{} from tick {}.",
                signature.numerator, signature.denominator, bar_start
            );
        }
    }

    /// Opens the Automation Editor (Tempo/Volume/Pan), pre-selecting the
    /// tempo point nearest the edit cursor if one exists exactly there.
    fn open_automation_editor(&mut self) {
        let at_cursor = self
            .project
            .tempo_points
            .iter()
            .find(|point| point.at_tick == self.cursor.tick)
            .map(|point| point.at_tick);
        self.automation_editor = Some(AutomationEditorState::new(at_cursor));
    }

    /// Insert a new bar to the left of the selected bar (`Ctrl+Ins` /
    /// `Ctrl+Shift+B`). Only affects the currently selected track.
    fn insert_bar_before_selected(&mut self) {
        let track_id = self.selected_track_id;
        let at_tick = self.cursor.tick;
        let previous_project = self.project.clone();
        if self.project.insert_bar_before(track_id, at_tick) {
            self.record_project_edit(previous_project);
            let bar_index = self
                .project
                .track(track_id)
                .map(|track| track.bar_index_at(self.cursor.tick))
                .unwrap_or_default();
            self.refresh_after_edit(track_id);
            self.status = format!("Inserted a bar before bar {}.", bar_index + 1);
        }
    }

    /// Add a new, empty bar to the right of the selected bar (`Ctrl+B`).
    /// Only affects the currently selected track.
    fn add_bar_after_selected(&mut self) {
        let track_id = self.selected_track_id;
        let at_tick = self.cursor.tick;
        let previous_project = self.project.clone();
        if self.project.add_bar_after(track_id, at_tick) {
            self.record_project_edit(previous_project);
            let bar_index = self
                .project
                .track(track_id)
                .map(|track| track.bar_index_at(self.cursor.tick))
                .unwrap_or_default();
            self.refresh_after_edit(track_id);
            self.status = format!("Added a bar after bar {}.", bar_index + 1);
        }
    }

    /// Delete the selected bar and its notes (`Ctrl+Del`). Refuses to remove
    /// a track's last remaining bar.
    fn delete_selected_bar(&mut self) {
        let track_id = self.selected_track_id;
        let at_tick = self.cursor.tick;
        let previous_project = self.project.clone();
        if self.project.delete_bar(track_id, at_tick) {
            self.record_project_edit(previous_project);
            self.selected_note_id = None;
            if let Some(track) = self.project.track(track_id) {
                let last_valid_tick = track.bars_end_tick().saturating_sub(1).max(0);
                self.cursor.tick = self.cursor.tick.min(last_valid_tick).max(0);
            }
            self.refresh_after_edit(track_id);
            self.status = "Deleted the selected bar.".to_owned();
        } else {
            self.status = "Can't delete a track's last remaining bar.".to_owned();
        }
    }

    fn delete_selected_note(&mut self) -> bool {
        let Some(note_id) = self.selected_note_id else {
            return false;
        };
        let previous_project = self.project.clone();
        if self.project.delete_note(self.selected_track_id, note_id) {
            self.record_project_edit(previous_project);
            self.selected_note_id = None;
            self.refresh_after_edit(self.selected_track_id);
            self.status = format!("Deleted note {note_id}.");
            true
        } else {
            false
        }
    }

    fn adjust_selected_fret(&mut self, delta: i16) -> bool {
        let Some(note_id) = self.selected_note_id else {
            return false;
        };
        let previous_project = self.project.clone();
        let changed = self
            .project
            .track_mut(self.selected_track_id)
            .and_then(|track| {
                let max_fret = track.fret_count;
                let note = track.notes.iter_mut().find(|note| note.id == note_id)?;
                let fret = (note.fret as i16 + delta).clamp(0, max_fret as i16) as u8;
                (fret != note.fret).then(|| {
                    note.fret = fret;
                    fret
                })
            });
        if let Some(fret) = changed {
            self.record_project_edit(previous_project);
            self.refresh_after_edit(self.selected_track_id);
            self.status = format!("Changed selected note to fret {fret}.");
            true
        } else {
            false
        }
    }

    fn step_selected_note_duration(&mut self, steps: i32) -> bool {
        let Some(note_id) = self.selected_note_id else {
            return false;
        };
        let Some(current_ticks) = self
            .project
            .track(self.selected_track_id)
            .and_then(|track| track.notes.iter().find(|note| note.id == note_id))
            .map(|note| note.duration_ticks)
        else {
            return false;
        };
        let current_index = DurationChoice::ALL
            .iter()
            .position(|duration| duration.ticks() == current_ticks)
            .unwrap_or_else(|| {
                DurationChoice::ALL
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, duration)| (duration.ticks() - current_ticks).abs())
                    .map(|(index, _)| index)
                    .unwrap_or(2)
            }) as i32;
        let new_index =
            (current_index + steps).clamp(0, DurationChoice::ALL.len() as i32 - 1) as usize;
        let duration = DurationChoice::ALL[new_index];
        let previous_project = self.project.clone();
        if self
            .project
            .set_note_duration_fluid(self.selected_track_id, note_id, duration.ticks())
        {
            self.selected_duration = duration;
            self.record_project_edit(previous_project);
            self.refresh_after_edit(self.selected_track_id);
            self.status = format!("Changed selected note length to {}.", duration.label());
            true
        } else {
            false
        }
    }

    /// If a fret digit has been typed and ~0.3s has passed with no further
    /// digit, auto-commits it as a note — matching Guitar Pro, so a
    /// multi-digit fret doesn't need an explicit Enter press. Must run every
    /// frame (not just on input) for the delay to actually elapse while the
    /// user does nothing else; schedules a repaint for the deadline itself,
    /// since egui doesn't repaint on a timer by default.
    fn auto_commit_pending_fret(&mut self, ctx: &egui::Context) {
        const AUTO_COMMIT_DELAY: std::time::Duration = std::time::Duration::from_millis(300);
        if self.cursor.pending_fret.is_none() {
            return;
        }
        let Some(typed_at) = self.cursor.pending_at else {
            return;
        };
        let elapsed = typed_at.elapsed();
        if elapsed >= AUTO_COMMIT_DELAY {
            self.commit_fret_at_cursor();
        } else {
            ctx.request_repaint_after(AUTO_COMMIT_DELAY - elapsed);
        }
    }

    /// If the pending fret buffer has gone stale (~1.2s without input), drop it
    /// so a stray `1` doesn't quietly prepend to the next number. In normal
    /// use `auto_commit_pending_fret` (0.3s) fires well before this ever
    /// would; this stays as a defensive fallback.
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
            self.status = format!("Fret buffer: {}  (Enter to place, Esc to clear)", candidate);
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
        let tick = self.cursor.tick.max(0);
        if tick >= track.bars_end_tick() {
            self.cursor.pending_fret = None;
            self.status =
                "No bar there yet — add a bar first (Ctrl+B or right-click > Bar > Add Bar)."
                    .to_owned();
            return;
        }
        let string_index = self
            .cursor
            .string_index
            .min(track.tuning.len().saturating_sub(1));
        let fret = self.cursor.pending_fret.unwrap_or(self.fret_to_insert);
        let duration = self.selected_duration.ticks();
        let previous_project = self.project.clone();

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
        } else if let Some(id) =
            self.project
                .insert_note(track_id, tick, duration, string_index, fret)
        {
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
        self.cursor.pending_at = None;
        self.record_project_edit(previous_project);
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
        let previous_project = self.project.clone();
        if let Some(id) = target
            && self.project.delete_note(track_id, id)
        {
            self.record_project_edit(previous_project);
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
    ///   - Ctrl+Z / Ctrl+Y (or Cmd+Z / Cmd+Shift+Z): undo / redo
    ///   - Esc: clear the pending fret buffer
    ///   - Home: snap cursor to tick 0
    fn handle_keyboard(&mut self, ctx: &egui::Context) -> bool {
        self.expire_pending_fret();
        self.auto_commit_pending_fret(ctx);
        let mut changed = false;

        ctx.input(|i| {
            // Use egui's platform-aware `command` modifier: Ctrl on Windows
            // and Linux, Command on macOS.
            if i.modifiers.command && i.key_pressed(egui::Key::Z) {
                if i.modifiers.shift {
                    self.redo();
                } else {
                    self.undo();
                }
                changed = true;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::Y) {
                self.redo();
                changed = true;
            }

            // Bar management. `Ctrl+Ins` and `Ctrl+Shift+B` both insert a bar
            // to the left of the selected bar; plain `Ctrl+B` adds one to the
            // right; `Ctrl+Del` removes the selected bar.
            if i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::B) {
                self.insert_bar_before_selected();
                changed = true;
            } else if i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::B) {
                self.add_bar_after_selected();
                changed = true;
            }
            if i.modifiers.ctrl && i.key_pressed(egui::Key::Insert) {
                self.insert_bar_before_selected();
                changed = true;
            }
            if i.modifiers.ctrl && i.key_pressed(egui::Key::Delete) {
                self.delete_selected_bar();
                changed = true;
            }

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

            // Shift+arrows adjust the selected brick. Plain arrows retain the
            // TuxGuitar-style cursor navigation behaviour.
            let adjust_selected_note = i.modifiers.shift && self.selected_note_id.is_some();
            if adjust_selected_note && i.key_pressed(egui::Key::ArrowRight) {
                changed |= self.step_selected_note_duration(-1);
            } else if i.key_pressed(egui::Key::ArrowRight) {
                self.advance_cursor();
                self.selected_note_id = None;
                changed = true;
            }
            if adjust_selected_note && i.key_pressed(egui::Key::ArrowLeft) {
                changed |= self.step_selected_note_duration(1);
            } else if i.key_pressed(egui::Key::ArrowLeft) {
                let step = self.selected_duration.ticks();
                self.cursor.tick = (self.cursor.tick - step).max(0);
                self.cursor.pending_fret = None;
                self.cursor.pending_at = None;
                self.selected_note_id = None;
                changed = true;
            }
            if adjust_selected_note && i.key_pressed(egui::Key::ArrowUp) {
                changed |= self.adjust_selected_fret(1);
            } else if i.key_pressed(egui::Key::ArrowUp) {
                self.move_cursor_string(1);
                changed = true;
            }
            if adjust_selected_note && i.key_pressed(egui::Key::ArrowDown) {
                changed |= self.adjust_selected_fret(-1);
            } else if i.key_pressed(egui::Key::ArrowDown) {
                self.move_cursor_string(-1);
                changed = true;
            }

            // Commit / delete
            if i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space) {
                self.commit_fret_at_cursor();
                changed = true;
            }
            if i.key_pressed(egui::Key::Backspace)
                || (i.key_pressed(egui::Key::Delete) && !i.modifiers.ctrl)
            {
                if self.cursor.pending_fret.is_some() {
                    self.cursor.pending_fret = None;
                    self.cursor.pending_at = None;
                    self.status = "Cleared pending fret.".to_owned();
                } else if !self.area_selected_note_ids.is_empty() {
                    self.delete_area_selected_notes();
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
        self.resolve_theme(root.ctx());

        match self.screen {
            AppScreen::StartMenu => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(self.theme.sheet_background))
                    .show_inside(root, |ui| self.show_start_menu(ui));
            }
            AppScreen::Editor => {
                // Pump keyboard input before any panel owns focus, so the canvas
                // acts like a DAW piano-roll: keys write notes even while a menu
                // is closed. Skipped while the New Project dialog is open so
                // typing a project name doesn't also feed the fret-entry buffer.
                if self.new_project_dialog.is_none() {
                    self.handle_keyboard(root.ctx());
                }

                egui::Panel::top("menu")
                    .exact_size(26.0)
                    .frame(egui::Frame::NONE.fill(self.theme.menu_bar_background))
                    .show_inside(root, |ui| self.show_menu(ui));

                egui::Panel::top("toolbar")
                    .exact_size(42.0)
                    .frame(egui::Frame::NONE.fill(self.theme.panel_background))
                    .show_inside(root, |ui| self.show_transport_toolbar(ui));

                egui::Panel::left("palette")
                    .resizable(false)
                    .exact_size(164.0)
                    .frame(egui::Frame::NONE.fill(self.theme.palette_background))
                    .show_inside(root, |ui| self.show_tux_palette(ui));

                egui::Panel::bottom("fretboard")
                    .exact_size(112.0)
                    .frame(egui::Frame::NONE.fill(self.theme.fretboard_background))
                    .show_inside(root, |ui| self.show_fretboard(ui));

                egui::Panel::bottom("track_table")
                    .exact_size(78.0)
                    .frame(egui::Frame::NONE.fill(self.theme.track_table_background))
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

        // Floats above either screen: reachable from the Start Menu's "New
        // Project..." button as well as the Editor's File > New Project...
        self.show_new_project_dialog(root.ctx());
        self.show_automation_editor(root.ctx());
    }
}

impl BetterWriterApp {
    fn show_menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Project...").clicked() {
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
                if ui.button("Start Menu...").clicked() {
                    self.return_to_start_menu();
                    ui.close();
                }
                ui.separator();
                let compatible = self.compatibility.standard_exports_enabled;
                let hint =
                    self.compatibility.warning.clone().unwrap_or_else(|| {
                        "Export each track as a standard tab bundle.".to_owned()
                    });
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
            ui.menu_button("Edit", |ui| {
                let can_undo = !self.undo_stack.is_empty();
                let can_redo = !self.redo_stack.is_empty();
                if ui
                    .add_enabled(can_undo, egui::Button::new("Undo\tCtrl+Z"))
                    .clicked()
                {
                    self.undo();
                    ui.close();
                }
                if ui
                    .add_enabled(can_redo, egui::Button::new("Redo\tCtrl+Y"))
                    .clicked()
                {
                    self.redo();
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                ui.label(egui::RichText::new("Theme").strong());
                for mode in AppThemeMode::ALL {
                    if ui
                        .radio_value(&mut self.theme_mode, mode, mode.label())
                        .clicked()
                    {
                        ui.close();
                    }
                }
            });
            for name in [
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
                ui.label("Ctrl+Z / Ctrl+Y ... undo / redo (Cmd+Z / Cmd+Shift+Z on macOS)");
                ui.label(
                    "Shift+arrows ..... adjust selected note: up/down fret, left/right length",
                );
                ui.label("Esc ............. clear pending fret / selection");
                ui.label("Home ............ move cursor to tick 0");
                ui.separator();
                ui.label(egui::RichText::new("Keyboard — Bars").strong());
                ui.label("Ctrl+Ins / Ctrl+Shift+B ... insert a bar before the selected bar");
                ui.label("Ctrl+B .......... add a bar after the selected bar");
                ui.label("Ctrl+Del ........ delete the selected bar");
                ui.separator();
                ui.label(egui::RichText::new("Mouse — Staff canvas").strong());
                ui.label("Click a note ...... select it");
                ui.label("Click empty space . move the keyboard cursor there");
                ui.label("Drag empty space .. rubber-band select several notes at once");
                ui.label("Delete/Backspace .. delete the selection (single note or several)");
                ui.label(
                    "Drag a selected note's left/right edge ... stretch it \
                    (step size: RMB > Note > Stretching)",
                );
                ui.label(
                    "Drag the middle of a selected note ......... move it in time and/or string \
                    (step size: RMB > Note > Dragging; hold Shift to disable snapping)",
                );
                ui.separator();
                ui.label("Tip: click the staff to reposition the cursor, then type a fret.");
            });
        });
    }

    /// The landing screen shown at launch and reachable via File > Start
    /// Menu... / the toolbar's Home button. Lists recently opened projects
    /// (double-click to open) alongside New Project / Demo Project actions.
    fn show_start_menu(&mut self, ui: &mut egui::Ui) {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.heading(egui::RichText::new("BetterWriter").size(32.0));
            ui.label("A tablature writer, built to be better.");
        });
        ui.add_space(28.0);

        let column_height = (ui.available_height() - 16.0).max(160.0);

        ui.horizontal(|ui| {
            ui.add_space(ui.available_width() * 0.12);

            ui.vertical(|ui| {
                ui.set_width(380.0);
                ui.set_min_height(column_height);

                ui.label(egui::RichText::new("Recent projects").strong());
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut self.start_menu_search);
                    if !self.start_menu_search.is_empty() && ui.button("×").clicked() {
                        self.start_menu_search.clear();
                    }
                });
                ui.add_space(4.0);
                if ui.button("Open other project...").clicked() {
                    self.open_from_explorer();
                }
                ui.separator();

                if self.recent_projects.is_empty() {
                    ui.label(
                        egui::RichText::new("No recent projects yet.")
                            .color(egui::Color32::from_gray(120)),
                    );
                } else {
                    // Re-sort (never filter) by match quality while the
                    // search box has text: primarily by how many query
                    // characters matched *in order* (a subsequence match),
                    // then by raw character overlap as a tiebreaker.
                    let mut ordered: Vec<PathBuf> = self.recent_projects.clone();
                    let query = self.start_menu_search.trim();
                    if !query.is_empty() {
                        ordered.sort_by(|a, b| {
                            let a_name = a.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                            let b_name = b.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                            search_match_score(query, b_name)
                                .cmp(&search_match_score(query, a_name))
                        });
                    }

                    let mut to_open: Option<PathBuf> = None;
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for path in &ordered {
                                let name = path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("Untitled");
                                let response = ui
                                    .add(
                                        egui::Label::new(format!(
                                            "{name}\n{}",
                                            path.display()
                                        ))
                                        .sense(Sense::click()),
                                    )
                                    .on_hover_text("Double-click to open");
                                if response.double_clicked() {
                                    to_open = Some(path.clone());
                                }
                                ui.separator();
                            }
                        });
                    if let Some(path) = to_open {
                        self.open_project_path(&path);
                    }
                }
            });

            ui.add_space(48.0);

            ui.vertical(|ui| {
                ui.set_width(220.0);
                ui.set_min_height(column_height);

                ui.label(egui::RichText::new("Start").strong());
                ui.separator();
                if ui
                    .add_sized([200.0, 32.0], egui::Button::new("New Project..."))
                    .clicked()
                {
                    self.new_project();
                }
                ui.add_space(6.0);
                if ui
                    .add_sized([200.0, 32.0], egui::Button::new("Demo Project"))
                    .on_hover_text("Open the built-in example score")
                    .clicked()
                {
                    self.load_demo_project();
                }
            });
        });
    }

    /// The floating "New Project" window: name, first-track instrument
    /// family/variant, and string count. Renders on top of whichever screen
    /// is active whenever `self.new_project_dialog` is `Some`.
    fn show_new_project_dialog(&mut self, ctx: &egui::Context) {
        let mut create = false;
        let mut cancel = false;

        if let Some(draft) = self.new_project_dialog.as_mut() {
            egui::Window::new("New Project")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_width(380.0);
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut draft.name);
                    });

                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("First track instrument").strong());
                    ui.horizontal(|ui| {
                        for family in InstrumentFamily::ALL {
                            let implemented = family.is_implemented();
                            ui.add_enabled_ui(implemented, |ui| {
                                if ui
                                    .selectable_label(draft.family == family, family.label())
                                    .on_hover_text(if implemented {
                                        "Available now"
                                    } else {
                                        "Coming soon"
                                    })
                                    .clicked()
                                {
                                    draft.family = family;
                                }
                            });
                        }
                    });

                    if draft.family == InstrumentFamily::Stringed {
                        ui.add_space(10.0);
                        ui.label("Stringed type");
                        ui.horizontal(|ui| {
                            for variant in StringedVariant::ALL {
                                if ui
                                    .selectable_label(
                                        draft.stringed_variant == variant,
                                        variant.label(),
                                    )
                                    .clicked()
                                {
                                    draft.stringed_variant = variant;
                                    draft.string_count = variant.default_string_count() as u32;
                                }
                            }
                        });

                        ui.add_space(10.0);
                        ui.label("Strings");
                        ui.horizontal_wrapped(|ui| {
                            for count in [4u32, 5, 6, 7, 8, 9, 10, 11, 12] {
                                if ui
                                    .selectable_label(
                                        draft.string_count == count,
                                        count.to_string(),
                                    )
                                    .clicked()
                                {
                                    draft.string_count = count;
                                }
                            }
                            ui.add(
                                egui::DragValue::new(&mut draft.string_count)
                                    .range(1..=64)
                                    .prefix("custom: "),
                            );
                        });
                    }

                    ui.add_space(14.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            create = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
        }

        if create {
            self.create_new_project_from_dialog();
        } else if cancel {
            self.new_project_dialog = None;
        }
    }

    /// The floating "Automation Editor" window (Tempo / Volume / Pan),
    /// styled after Guitar Pro 8's. `self.automation_editor` is taken out for
    /// the duration of this call so its contents can freely call back into
    /// `&mut self` (project edits, undo) without fighting the borrow
    /// checker, then put back unless the window was closed.
    fn show_automation_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.automation_editor.take() else {
            return;
        };
        let mut keep_open = true;

        egui::Window::new("Automation Editor")
            .id(egui::Id::new("automation_editor_window"))
            .open(&mut keep_open)
            .default_size([760.0, 480.0])
            .min_width(560.0)
            .min_height(360.0)
            .resizable(true)
            .show(ctx, |ui| {
                self.show_automation_editor_contents(ui, &mut state);
            });

        if keep_open {
            // Delete/Backspace removes the selected point(s) — but only when
            // no widget has keyboard focus, so backspacing while typing into
            // the bpm field doesn't also delete the point.
            let any_widget_focused = ctx.memory(|memory| memory.focused().is_some());
            if !any_widget_focused
                && !state.selected_points.is_empty()
                && ctx.input(|input| {
                    input.key_pressed(egui::Key::Delete)
                        || input.key_pressed(egui::Key::Backspace)
                })
            {
                let before = self.project.clone();
                let mut any = false;
                for tick in state.selected_points.clone() {
                    if self.project.delete_tempo_point(tick) {
                        any = true;
                    }
                }
                if any {
                    self.record_project_edit(before);
                }
                state.selected_points.clear();
            }
            self.automation_editor = Some(state);
        }
    }

    fn show_automation_editor_contents(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut AutomationEditorState,
    ) {
        // Without this, the window snaps back to its content's natural
        // (small) height on every frame, which is what made the resize
        // handle appear to only work horizontally: egui shrink-wraps a
        // Window's height to whatever its content actually uses unless the
        // content explicitly claims the space a manual resize gave it.
        ui.set_min_height(ui.available_height());

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("TYPE:").strong());
            egui::ComboBox::from_id_salt("automation_type")
                .selected_text(state.automation_type.label())
                .show_ui(ui, |ui| {
                    for kind in AutomationType::ALL {
                        ui.add_enabled_ui(kind.is_implemented(), |ui| {
                            if ui
                                .selectable_label(state.automation_type == kind, kind.label())
                                .on_hover_text(if kind.is_implemented() {
                                    "Available now"
                                } else {
                                    "Coming soon"
                                })
                                .clicked()
                            {
                                state.automation_type = kind;
                            }
                        });
                    }
                });
            ui.add_space(16.0);
            if state.automation_type == AutomationType::Tempo
                && ui
                    .button("Remove automations")
                    .on_hover_text("Reset to a single flat tempo for the whole project")
                    .clicked()
            {
                let before = self.project.clone();
                let flat_bpm = self.project.tempo_at(0);
                self.project.clear_tempo_automation(flat_bpm);
                self.record_project_edit(before);
                state.selected_points.clear();
            }
        });
        ui.separator();

        match state.automation_type {
            AutomationType::Tempo => {
                let body_height = ui.available_height().max(220.0);
                ui.horizontal(|ui| {
                    // `top_down` here is load-bearing, not decorative: a
                    // plain `ui.allocate_ui` inherits this row's
                    // left-to-right layout, which disables text wrapping —
                    // that's what was making the point-settings text spill
                    // out past its column instead of wrapping/stacking, and
                    // in turn made the graph column end up far smaller than
                    // it should've been.
                    ui.allocate_ui_with_layout(
                        Vec2::new(210.0, body_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            self.show_tempo_point_settings(ui, state);
                        },
                    );
                    ui.separator();
                    let graph_width = ui.available_width();
                    ui.allocate_ui_with_layout(
                        Vec2::new(graph_width, body_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            self.show_tempo_graph(ui, state, body_height);
                        },
                    );
                });
            }
            AutomationType::Volume | AutomationType::Pan => {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.label(format!(
                        "{} automation is coming soon.",
                        state.automation_type.label()
                    ));
                });
            }
        }
    }

    fn show_tempo_point_settings(&mut self, ui: &mut egui::Ui, state: &mut AutomationEditorState) {
        ui.label(egui::RichText::new("POINT SETTINGS").strong());
        ui.separator();

        let selected_count = state.selected_points.len();
        let single_point = if selected_count == 1 {
            let tick = state.selected_points[0];
            self.project
                .tempo_points
                .iter()
                .find(|point| point.at_tick == tick)
                .copied()
        } else {
            None
        };
        // A stale selection (e.g. the point got deleted from under it) —
        // drop it and fall through to the "0 selected" presentation.
        if selected_count == 1 && single_point.is_none() {
            state.selected_points.clear();
        }
        let selected_count = state.selected_points.len();

        ui.label("Position:");
        ui.label(
            egui::RichText::new(match selected_count {
                0 => "—".to_owned(),
                1 => format!("tick {}", single_point.unwrap().at_tick),
                n => format!("{n} points selected"),
            })
            .color(egui::Color32::from_gray(180)),
        );

        ui.add_space(8.0);
        ui.label("Tempo:");
        ui.horizontal_wrapped(|ui| {
            let mut bpm = single_point
                .map(|point| point.bpm)
                .unwrap_or(self.project.interpolated_tempo_at(self.cursor.tick));
            ui.add_enabled_ui(selected_count == 1, |ui| {
                let dragged = ui
                    .add(
                        egui::DragValue::new(&mut bpm)
                            .range(20.0..=320.0)
                            .suffix(" bpm"),
                    )
                    .changed();
                let mut tapped_bpm = None;
                if ui
                    .button("TAP")
                    .on_hover_text("Click in rhythm to set a tempo")
                    .clicked()
                {
                    let now = std::time::Instant::now();
                    state
                        .tap_times
                        .retain(|tap| now.duration_since(*tap).as_secs_f32() < 2.0);
                    state.tap_times.push(now);
                    if state.tap_times.len() >= 2 {
                        let span = state
                            .tap_times
                            .last()
                            .unwrap()
                            .duration_since(*state.tap_times.first().unwrap())
                            .as_secs_f32();
                        let intervals = state.tap_times.len() as f32 - 1.0;
                        if span > 0.0 {
                            tapped_bpm = Some((60.0 * intervals / span).clamp(20.0, 320.0));
                        }
                    }
                }
                if let Some(tapped_bpm) = tapped_bpm {
                    bpm = tapped_bpm;
                }
                if (dragged || tapped_bpm.is_some()) && let Some(point) = single_point {
                    let before = self.project.clone();
                    if self.project.set_tempo_point(point.at_tick, bpm, point.transition) {
                        self.record_project_edit(before);
                    }
                }
            });
        });
        if selected_count == 0 {
            ui.label(
                egui::RichText::new(format!(
                    "(effective tempo at cursor, tick {})",
                    self.cursor.tick
                ))
                .small()
                .color(egui::Color32::from_gray(140)),
            );
        }

        ui.add_space(8.0);
        ui.label("Transition:");
        self.show_tempo_transition_brush(ui, state);

        ui.add_space(8.0);
        ui.add_enabled_ui(selected_count == 1, |ui| {
            let mut hidden = single_point.map(|point| point.hidden).unwrap_or(false);
            if ui.checkbox(&mut hidden, "Hide automation label").changed()
                && let Some(point) = single_point
            {
                let before = self.project.clone();
                if self.project.set_tempo_point_hidden(point.at_tick, hidden) {
                    self.record_project_edit(before);
                }
            }
        });

        ui.add_space(10.0);
        let can_delete = selected_count > 1
            || (selected_count == 1 && single_point.map(|point| point.at_tick) != Some(0));
        ui.add_enabled_ui(can_delete, |ui| {
            let label = if selected_count > 1 {
                "Delete selected points"
            } else {
                "Delete point"
            };
            if ui
                .button(label)
                .on_hover_text(if selected_count == 1 && !can_delete {
                    "The first point can't be removed — it sets the project's starting tempo"
                } else {
                    "Remove the selected point(s)"
                })
                .clicked()
            {
                let before = self.project.clone();
                let mut any = false;
                for tick in state.selected_points.clone() {
                    if self.project.delete_tempo_point(tick) {
                        any = true;
                    }
                }
                if any {
                    self.record_project_edit(before);
                }
                state.selected_points.clear();
            }
        });

        ui.add_space(10.0);
        ui.separator();
        ui.label(
            egui::RichText::new(
                "Click a point to select it, drag a rectangle to select \
                several, or click empty space to add a new point.",
            )
            .small()
            .color(egui::Color32::from_gray(140)),
        );
    }

    /// The Constant/Progressive transition control. Always visible — not
    /// nested inside a "1 point selected" branch — because it does double
    /// duty as both a "brush" (the transition new points are created with)
    /// and, while a selection exists, an action that applies to it. It never
    /// resets itself to reflect whatever's currently selected: it's a
    /// persistent setting, not a readout.
    fn show_tempo_transition_brush(&mut self, ui: &mut egui::Ui, state: &mut AutomationEditorState) {
        let previous = state.default_transition;
        ui.radio_value(
            &mut state.default_transition,
            TempoTransition::Constant,
            "Constant until next point",
        );
        ui.radio_value(
            &mut state.default_transition,
            TempoTransition::Progressive,
            "Progressive to next point",
        );
        if state.default_transition != previous && !state.selected_points.is_empty() {
            let transition = state.default_transition;
            let before = self.project.clone();
            let mut any = false;
            for tick in state.selected_points.clone() {
                let bpm = self.project.tempo_at(tick);
                if self.project.set_tempo_point(tick, bpm, transition) {
                    any = true;
                }
            }
            if any {
                self.record_project_edit(before);
            }
        }
    }

    /// The scrollable tempo curve: bar gridlines from the selected track, a
    /// step/ramp line through every tempo point, and draggable point
    /// handles. Points can be area-selected (drag from empty space) and then
    /// moved or deleted together.
    fn show_tempo_graph(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut AutomationEditorState,
        available_height: f32,
    ) {
        const MIN_BPM: f32 = 20.0;
        const MAX_BPM: f32 = 320.0;
        const MIN_GRAPH_HEIGHT: f32 = 160.0;
        const POINT_HIT_RADIUS: f32 = 8.0;
        const POINT_DRAW_RADIUS: f32 = 5.0;

        let zoom_row = ui.horizontal(|ui| {
            ui.label("Zoom:");
            ui.add(egui::Slider::new(&mut state.pixels_per_tick, 0.01..=0.2));
            ui.separator();
            ui.checkbox(&mut state.snap_to_grid, "Snap to grid");
        });
        ui.add_space(4.0);
        // Explicitly passed down from `show_automation_editor_contents`
        // (rather than read via `ui.available_height()` here) and measured
        // against the zoom row's *actual* drawn height, so the graph
        // reliably fills whatever room the window currently has — resizing
        // the window taller gives a taller graph — instead of shrinking to
        // some small fixed size regardless of the window.
        let graph_height =
            (available_height - zoom_row.response.rect.height() - 8.0).max(MIN_GRAPH_HEIGHT);

        let horizon_ticks = self
            .project
            .tracks
            .iter()
            .map(|track| track.bars_end_tick())
            .max()
            .unwrap_or(TICKS_PER_QUARTER * 4)
            .max(
                self.project
                    .tempo_points
                    .iter()
                    .map(|point| point.at_tick)
                    .max()
                    .unwrap_or(0)
                    + TICKS_PER_QUARTER * 4,
            )
            + TICKS_PER_QUARTER * 4;
        let pixels_per_tick = state.pixels_per_tick;
        let content_width = (horizon_ticks as f32 * pixels_per_tick).max(200.0);

        egui::ScrollArea::both()
            .id_salt("tempo_automation_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (response, painter) = ui.allocate_painter(
                    Vec2::new(content_width, graph_height),
                    Sense::click_and_drag(),
                );
                let rect = response.rect;
                // Local copies, not live reads through `state`, so these
                // little mapping closures never hold a borrow of `state` —
                // the interaction handlers below need to freely read *and*
                // write several other `state` fields in the same scope.
                let snap_to_grid = state.snap_to_grid;

                let bpm_to_y = |bpm: f32| {
                    let t = ((bpm - MIN_BPM) / (MAX_BPM - MIN_BPM)).clamp(0.0, 1.0);
                    rect.bottom() - t * rect.height()
                };
                let y_to_bpm = |y: f32| {
                    let t = ((rect.bottom() - y) / rect.height()).clamp(0.0, 1.0);
                    MIN_BPM + t * (MAX_BPM - MIN_BPM)
                };
                let tick_to_x = |tick: Tick| rect.left() + tick as f32 * pixels_per_tick;
                let x_to_tick =
                    |x: f32| (((x - rect.left()) / pixels_per_tick).max(0.0)) as Tick;

                painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(24, 24, 28));

                if let Some(track) = self.project.track(self.selected_track_id) {
                    let mut cursor: Tick = 0;
                    for _ in 0..track.bar_count.max(1) {
                        let (start, end) = track.measure_bounds_at(cursor);
                        let x = tick_to_x(start);
                        painter.line_segment(
                            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                            Stroke::new(1.0, egui::Color32::from_gray(50)),
                        );
                        cursor = end;
                    }
                }

                let mut bpm_line = (MIN_BPM / 20.0).ceil() * 20.0;
                while bpm_line <= MAX_BPM {
                    let y = bpm_to_y(bpm_line);
                    painter.line_segment(
                        [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                        Stroke::new(1.0, egui::Color32::from_gray(40)),
                    );
                    painter.text(
                        Pos2::new(rect.left() + 2.0, y),
                        Align2::LEFT_BOTTOM,
                        format!("{bpm_line:.0}"),
                        FontId::monospace(9.0),
                        egui::Color32::from_gray(140),
                    );
                    bpm_line += 20.0;
                }

                let mut sorted_points = self.project.tempo_points.clone();
                sorted_points.sort_by_key(|point| point.at_tick);

                for pair in sorted_points.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    let (ax, ay) = (tick_to_x(a.at_tick), bpm_to_y(a.bpm));
                    let (bx, by) = (tick_to_x(b.at_tick), bpm_to_y(b.bpm));
                    match a.transition {
                        TempoTransition::Progressive => {
                            painter.line_segment(
                                [Pos2::new(ax, ay), Pos2::new(bx, by)],
                                Stroke::new(2.0, self.theme.accent_color),
                            );
                        }
                        TempoTransition::Constant => {
                            painter.line_segment(
                                [Pos2::new(ax, ay), Pos2::new(bx, ay)],
                                Stroke::new(2.0, self.theme.accent_color),
                            );
                            painter.line_segment(
                                [Pos2::new(bx, ay), Pos2::new(bx, by)],
                                Stroke::new(1.0, egui::Color32::from_gray(120)),
                            );
                        }
                    }
                }
                if let Some(last) = sorted_points.last() {
                    let (lx, ly) = (tick_to_x(last.at_tick), bpm_to_y(last.bpm));
                    painter.line_segment(
                        [Pos2::new(lx, ly), Pos2::new(rect.right(), ly)],
                        Stroke::new(1.0, egui::Color32::from_gray(90)),
                    );
                }

                for point in &sorted_points {
                    let center = Pos2::new(tick_to_x(point.at_tick), bpm_to_y(point.bpm));
                    let selected = state.selected_points.contains(&point.at_tick);
                    painter.circle(
                        center,
                        POINT_DRAW_RADIUS,
                        if selected {
                            self.theme.accent_color
                        } else {
                            egui::Color32::from_rgb(224, 224, 224)
                        },
                        Stroke::new(1.5, egui::Color32::BLACK),
                    );
                }

                let pointer_pos = response.interact_pointer_pos();
                let hit_test = |pos: Pos2| {
                    sorted_points
                        .iter()
                        .find(|point| {
                            let center = Pos2::new(tick_to_x(point.at_tick), bpm_to_y(point.bpm));
                            (center - pos).length() <= POINT_HIT_RADIUS
                        })
                        .map(|point| point.at_tick)
                };

                if response.drag_started() {
                    let start = pointer_pos.unwrap_or(rect.left_top());
                    state.drag_started_snapshot = Some(self.project.clone());
                    if let Some(hit_tick) = hit_test(start) {
                        if !state.selected_points.contains(&hit_tick) {
                            state.selected_points = vec![hit_tick];
                        }
                        let anchor_index = state
                            .selected_points
                            .iter()
                            .position(|tick| *tick == hit_tick)
                            .unwrap_or(0);
                        let original_points: Vec<(Tick, f32)> = state
                            .selected_points
                            .iter()
                            .map(|tick| {
                                self.project
                                    .tempo_points
                                    .iter()
                                    .find(|point| point.at_tick == *tick)
                                    .map(|point| (point.at_tick, point.bpm))
                                    .unwrap_or((*tick, 120.0))
                            })
                            .collect();
                        state.drag_mode = AutomationDragMode::MovingPoints {
                            original_points,
                            anchor_index,
                            raw_offset_ticks: 0.0,
                            raw_offset_bpm: 0.0,
                        };
                    } else {
                        state.drag_mode = AutomationDragMode::AreaSelecting {
                            start_pos: start,
                            current_pos: start,
                        };
                    }
                }

                if response.dragged() {
                    let delta = response.drag_delta();
                    match &mut state.drag_mode {
                        AutomationDragMode::MovingPoints {
                            original_points,
                            anchor_index,
                            raw_offset_ticks,
                            raw_offset_bpm,
                        } => {
                            *raw_offset_ticks += delta.x as f64 / pixels_per_tick as f64;
                            *raw_offset_bpm += -(delta.y / rect.height()) * (MAX_BPM - MIN_BPM);

                            // Only the grabbed point's raw (unsnapped)
                            // position decides whether the drag is
                            // "close enough" to a grid line right now —
                            // recomputed fresh each frame from the
                            // fixed drag-start baseline, so a small
                            // nudge can never compound into a runaway
                            // snap. Below the threshold, everyone just
                            // moves by the exact raw offset.
                            let (anchor_original_tick, _) = original_points[*anchor_index];
                            let anchor_raw_tick = anchor_original_tick as f64 + *raw_offset_ticks;
                            let effective_offset_ticks = if snap_to_grid {
                                let grid = (TICKS_PER_QUARTER / 4) as f64;
                                let nearest_grid_line = (anchor_raw_tick / grid).round() * grid;
                                const SNAP_THRESHOLD_PIXELS: f64 = 6.0;
                                let snap_threshold_ticks =
                                    SNAP_THRESHOLD_PIXELS / pixels_per_tick as f64;
                                let snapped_anchor_tick = if (anchor_raw_tick
                                    - nearest_grid_line)
                                    .abs()
                                    <= snap_threshold_ticks
                                {
                                    nearest_grid_line
                                } else {
                                    anchor_raw_tick.round()
                                };
                                snapped_anchor_tick - anchor_original_tick as f64
                            } else {
                                raw_offset_ticks.round()
                            };

                            let moving = state.selected_points.clone();
                            let mut updated = Vec::with_capacity(moving.len());
                            for (index, tick) in moving.iter().enumerate() {
                                let (original_tick, original_bpm) = original_points[index];
                                let is_anchor = original_tick == 0;
                                let new_tick = if is_anchor {
                                    0
                                } else {
                                    (original_tick + effective_offset_ticks as Tick).max(0)
                                };
                                let new_bpm =
                                    (original_bpm + *raw_offset_bpm).clamp(MIN_BPM, MAX_BPM);
                                if !is_anchor && new_tick != *tick {
                                    self.project
                                        .tempo_points
                                        .retain(|existing| existing.at_tick != *tick);
                                }
                                let transition = self
                                    .project
                                    .tempo_points
                                    .iter()
                                    .find(|point| point.at_tick == *tick)
                                    .map(|point| point.transition)
                                    .unwrap_or(state.default_transition);
                                let hidden = self
                                    .project
                                    .tempo_points
                                    .iter()
                                    .find(|point| point.at_tick == *tick)
                                    .map(|point| point.hidden)
                                    .unwrap_or(false);
                                self.project.set_tempo_point(new_tick, new_bpm, transition);
                                self.project.set_tempo_point_hidden(new_tick, hidden);
                                updated.push(new_tick);
                            }
                            state.selected_points = updated;
                        }
                        AutomationDragMode::AreaSelecting { start_pos, .. } => {
                            let current = pointer_pos.unwrap_or(*start_pos);
                            state.drag_mode = AutomationDragMode::AreaSelecting {
                                start_pos: *start_pos,
                                current_pos: current,
                            };
                        }
                        AutomationDragMode::Idle => {}
                    }
                }

                if response.drag_stopped() {
                    if let AutomationDragMode::AreaSelecting {
                        start_pos,
                        current_pos,
                    } = &state.drag_mode
                    {
                        let selection_rect = Rect::from_two_pos(*start_pos, *current_pos);
                        state.selected_points = sorted_points
                            .iter()
                            .filter(|point| {
                                let center =
                                    Pos2::new(tick_to_x(point.at_tick), bpm_to_y(point.bpm));
                                selection_rect.contains(center)
                            })
                            .map(|point| point.at_tick)
                            .collect();
                    }
                    state.drag_mode = AutomationDragMode::Idle;
                    if let Some(before) = state.drag_started_snapshot.take() {
                        self.record_project_edit(before);
                    }
                }

                if response.clicked() && !response.dragged() {
                    if let Some(pos) = pointer_pos {
                        if let Some(hit_tick) = hit_test(pos) {
                            state.selected_points = vec![hit_tick];
                        } else {
                            let mut tick = x_to_tick(pos.x);
                            if snap_to_grid {
                                tick = snap_tick(tick, TICKS_PER_QUARTER / 4);
                            }
                            let bpm = y_to_bpm(pos.y);
                            let before = self.project.clone();
                            self.project
                                .set_tempo_point(tick, bpm, state.default_transition);
                            self.record_project_edit(before);
                            state.selected_points = vec![tick];
                        }
                    }
                }

                // Right-clicking empty graph space clears the selection
                // (no context menu exists here yet, so this is the only
                // thing a right-click does).
                if response.secondary_clicked()
                    && let Some(pos) = pointer_pos
                    && hit_test(pos).is_none()
                {
                    state.selected_points.clear();
                }

                if let AutomationDragMode::AreaSelecting {
                    start_pos,
                    current_pos,
                } = &state.drag_mode
                {
                    let selection_rect = Rect::from_two_pos(*start_pos, *current_pos);
                    painter.rect_filled(
                        selection_rect,
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(80, 140, 255, 40),
                    );
                    painter.rect_stroke(
                        selection_rect,
                        0.0,
                        Stroke::new(1.0, egui::Color32::from_rgb(80, 140, 255)),
                        StrokeKind::Middle,
                    );
                }
            });
    }

    fn show_transport_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            if small_tool_button(ui, "Home", "Return to the start menu").clicked() {
                self.return_to_start_menu();
            }
            if small_tool_button(ui, "New", "New project").clicked() {
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
            if small_tool_button(
                ui,
                "TVP Automation",
                "Tempo / Volume / Pan automation editor",
            )
            .clicked()
            {
                self.open_automation_editor();
            }
            ui.label("duration");
            const DURATION_LABELS: [&str; 6] = ["1", "1/2", "1/4", "1/8", "1/16", "1/32"];
            if let Some(new_duration) = list_drag_value(
                ui,
                "toolbar_duration",
                &DurationChoice::ALL,
                &DURATION_LABELS,
                self.selected_duration,
            ) {
                self.selected_duration = new_duration;
            }
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
            if self.selected_note_mut().is_some() {
                if small_tool_button(ui, "Dur", "Apply selected duration to note").clicked() {
                    self.apply_selected_duration();
                }
                if small_tool_button(ui, "Del", "Delete selected note").clicked() {
                    self.delete_selected_note();
                }
            } else if !self.area_selected_note_ids.is_empty() {
                if small_tool_button(
                    ui,
                    "Dur",
                    "Apply selected duration to all selected notes",
                )
                .clicked()
                {
                    self.apply_selected_duration_to_area_selection();
                }
                if small_tool_button(
                    ui,
                    "Del",
                    "Delete all area-selected notes",
                )
                .clicked()
                {
                    self.delete_area_selected_notes();
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
                if small_tool_button(
                    ui,
                    "Bar+",
                    "Add a bar after the selected bar (Ctrl+B)",
                )
                .clicked()
                {
                    self.add_bar_after_selected();
                }
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
        painter.rect_filled(rect, 0.0, self.theme.fretboard_background);
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
            let previous_project = self.project.clone();
            if self.project.set_note_duration_fluid(
                track_id,
                note_id,
                self.selected_duration.ticks(),
            ) {
                self.record_project_edit(previous_project);
                self.refresh_after_edit(track_id);
                self.status = "Duration changed; later notes in the measure shifted.".to_owned();
            }
        }
    }

    /// Same as `apply_selected_duration`, but for every note in
    /// `area_selected_note_ids` at once, as a single undo step.
    fn apply_selected_duration_to_area_selection(&mut self) {
        if self.area_selected_note_ids.is_empty() {
            return;
        }
        let track_id = self.selected_track_id;
        let duration = self.selected_duration.ticks();
        let previous_project = self.project.clone();
        let mut changed_count = 0;
        for note_id in self.area_selected_note_ids.clone() {
            if self
                .project
                .set_note_duration_fluid(track_id, note_id, duration)
            {
                changed_count += 1;
            }
        }
        if changed_count > 0 {
            self.record_project_edit(previous_project);
            self.refresh_after_edit(track_id);
            self.status = format!(
                "Duration changed on {changed_count} notes; later notes in each measure shifted."
            );
        }
    }

    fn apply_velocity_to_selected(&mut self) {
        let Some(note_id) = self.selected_note_id else {
            return;
        };
        let previous_project = self.project.clone();
        if let Some(track) = self.project.track_mut(self.selected_track_id)
            && let Some(note) = track.notes.iter_mut().find(|note| note.id == note_id)
        {
            note.velocity = self.active_velocity;
            self.record_project_edit(previous_project);
            self.refresh_after_edit(self.selected_track_id);
        }
    }

    fn apply_effects_to_selected(&mut self) {
        let Some(note_id) = self.selected_note_id else {
            return;
        };
        let previous_project = self.project.clone();
        if let Some(track) = self.project.track_mut(self.selected_track_id)
            && let Some(note) = track.notes.iter_mut().find(|note| note.id == note_id)
        {
            note.effects = self.active_effects.clone();
            self.record_project_edit(previous_project);
            self.refresh_after_edit(self.selected_track_id);
        }
    }

    fn show_editor(&mut self, ui: &mut egui::Ui) {
        let desired = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, self.theme.canvas_backdrop);

        let page = rect.shrink2(Vec2::new(18.0, 18.0));
        painter.rect_filled(page, 0.0, self.theme.sheet_background);

        let track_rect = self.paint_selected_track_page(&painter, page);
        self.paint_shadow_overview(&painter, page);

        if response.drag_started()
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.begin_canvas_drag(pos, track_rect);
        }
        if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.update_canvas_drag(pos);
        }
        if response.drag_stopped() {
            let snapping_disabled = ui.input(|input| input.modifiers.shift);
            self.finish_canvas_drag(track_rect, snapping_disabled);
        }
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.handle_canvas_click(pos, track_rect);
        }
        if response.secondary_clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            // Right-click uses the same hit testing as a normal click, so the
            // menu always operates on the exact staff position or selected
            // note under the pointer.
            self.handle_canvas_click(pos, track_rect);
            self.sync_time_signature_picker();
        }
        // `PopupCloseBehavior::CloseOnClickOutside` (egui's default for
        // context menus is `CloseOnClick`, which closes on *any* click,
        // including on a widget inside the menu — that's what made
        // interactive submenu controls like Stretching's DragValue
        // unusable). Menu items that should still close on click (most
        // actions) do so via their own explicit `ui.close()` call.
        egui::Popup::context_menu(&response)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| self.show_canvas_context_menu(ui));

        // Change the cursor to a resize icon when hovering a stretchable
        // edge of a selected note, so it's discoverable without dragging.
        if matches!(self.canvas_drag, CanvasDragMode::Idle)
            && let Some(pos) = response.hover_pos()
            && self.stretch_edge_at(pos, track_rect).is_some()
        {
            ui.ctx()
                .set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }

        // Whatever the in-progress drag gesture is doing, drawn on top of
        // everything else.
        match &self.canvas_drag {
            CanvasDragMode::AreaSelecting {
                start_pos,
                current_pos,
            } => {
                let selection_rect = Rect::from_two_pos(*start_pos, *current_pos);
                painter.rect_filled(
                    selection_rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(80, 140, 255, 40),
                );
                painter.rect_stroke(
                    selection_rect,
                    0.0,
                    Stroke::new(1.0, egui::Color32::from_rgb(80, 140, 255)),
                    StrokeKind::Middle,
                );
            }
            CanvasDragMode::Stretching {
                start_pos,
                current_pos,
                ..
            } => {
                let raw_delta_ticks =
                    ((current_pos.x - start_pos.x) / self.pixels_per_tick).round() as Tick;
                let step = self.stretch_step_ticks.max(1);
                let snapped_delta_ticks = (raw_delta_ticks / step) * step;
                let preview_x = start_pos.x + snapped_delta_ticks as f32 * self.pixels_per_tick;
                painter.line_segment(
                    [
                        Pos2::new(preview_x, track_rect.top()),
                        Pos2::new(preview_x, track_rect.bottom()),
                    ],
                    Stroke::new(2.0, egui::Color32::from_rgb(80, 140, 255)),
                );
                painter.text(
                    Pos2::new(preview_x + 4.0, track_rect.top() + 2.0),
                    Align2::LEFT_TOP,
                    format!("{snapped_delta_ticks:+} ticks"),
                    FontId::monospace(11.0),
                    egui::Color32::from_rgb(40, 90, 200),
                );
            }
            CanvasDragMode::MovingNotes {
                anchor_note_id,
                start_pos,
                current_pos,
                ..
            } => {
                let snapping_disabled = ui.input(|input| input.modifiers.shift);
                let raw_delta_ticks =
                    (current_pos.x - start_pos.x) as f64 / self.pixels_per_tick as f64;
                let preview_offset_ticks = self.compute_move_snap_offset(
                    *anchor_note_id,
                    raw_delta_ticks,
                    snapping_disabled,
                );
                const STRING_GAP: f32 = 9.0;
                let string_delta = ((current_pos.y - start_pos.y) / STRING_GAP).round() as i32;
                let preview_x = start_pos.x + preview_offset_ticks as f32 * self.pixels_per_tick;
                painter.line_segment(
                    [
                        Pos2::new(preview_x, track_rect.top()),
                        Pos2::new(preview_x, track_rect.bottom()),
                    ],
                    Stroke::new(2.0, egui::Color32::from_rgb(255, 150, 60)),
                );
                let string_label = if string_delta == 0 {
                    String::new()
                } else {
                    format!(", string {string_delta:+}")
                };
                painter.text(
                    Pos2::new(preview_x + 4.0, track_rect.top() + 2.0),
                    Align2::LEFT_TOP,
                    format!(
                        "{preview_offset_ticks:+} ticks{string_label}{}",
                        if snapping_disabled { " (no snap)" } else { "" }
                    ),
                    FontId::monospace(11.0),
                    egui::Color32::from_rgb(200, 100, 20),
                );
            }
            CanvasDragMode::Idle => {}
        }
    }

    fn show_canvas_context_menu(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Staff context menu").strong());
        ui.label(format!(
            "Track: {} · tick {}",
            self.project
                .track(self.selected_track_id)
                .map(|track| track.name.as_str())
                .unwrap_or("No track"),
            self.cursor.tick
        ));
        ui.separator();
        planned_menu_item(ui, "Cut  (Ctrl+X)");
        planned_menu_item(ui, "Copy  (Ctrl+C)");
        planned_menu_item(ui, "Paste  (Ctrl+V)");
        planned_menu_item(ui, "All-Track Cut  (Ctrl+Shift+X)");
        planned_menu_item(ui, "All-Track Copy  (Ctrl+Shift+C)");
        planned_menu_item(ui, "Special Paste...  (Ctrl+Shift+V)");
        ui.separator();
        let has_selection = self.selected_note_id.is_some();
        let has_area_selection = !self.area_selected_note_ids.is_empty();
        submenu(ui, "Bar", |ui| {
            if ui.button("Insert Bar  (Ctrl+Ins)").clicked() {
                self.insert_bar_before_selected();
                ui.close();
            }
            if ui.button("Add Bar  (Ctrl+B)").clicked() {
                self.add_bar_after_selected();
                ui.close();
            }
            if ui.button("Delete Bar  (Ctrl+Del)").clicked() {
                self.delete_selected_bar();
                ui.close();
            }
            planned_menu_item(ui, "Clef...  (K)");
            planned_menu_item(ui, "Key Signature...  (Ctrl+K)");
            submenu(ui, "Time Signature...  (Ctrl+T)", |ui| {
                ui.label("Applies to the whole bar containing the click");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut self.time_signature_numerator)
                            .range(1..=32)
                            .prefix(" "),
                    );
                    ui.label("/");
                    // A list-scrolling `DragValue` here (rather than a
                    // `ComboBox`) sidesteps the same nested-popup-in-menu
                    // conflict ComboBox has, and lets the denominator be
                    // dragged/clicked through exactly like the numerator —
                    // just stepping through {1,2,4,8,16,32} instead of every
                    // integer.
                    const DENOMINATORS: [u8; 6] = [1, 2, 4, 8, 16, 32];
                    const DENOMINATOR_LABELS: [&str; 6] = ["1", "2", "4", "8", "16", "32"];
                    if let Some(new_denominator) = list_drag_value(
                        ui,
                        "context_time_signature_denominator",
                        &DENOMINATORS,
                        &DENOMINATOR_LABELS,
                        self.time_signature_denominator,
                    ) {
                        self.time_signature_denominator = new_denominator;
                    }
                });
                if ui.button("Apply to this track").clicked() {
                    self.apply_time_signature_at_cursor();
                    ui.close();
                }
            });
            for label in [
                "Triplet Feel...  (Ctrl+/)",
                "Free Time  (|)",
                "Double Barline",
                "Anacrusis (Pickup Bar)",
                "Repeat Open  ([)",
                "Alternate Endings...",
                "Repeat Close...  (])",
                "Directions...  (D)",
                "Simile Marks",
                "Multirest  (Ctrl+R)",
                "Force Break Line  (Ctrl+Return)",
                "Prevent Break Line",
                "System Layout...",
            ] {
                planned_menu_item(ui, label);
            }
        });
        let mut note_effects_changed = false;
        submenu(ui, "Note", |ui| {
            planned_menu_item(ui, "Insert a Beat  (Ins)");
            planned_menu_item(ui, "Delete the Beats  (Shift+Del)");
            planned_menu_item(ui, "Copy Beats at the End  (C)");
            submenu(ui, "Duration", |ui| {
                ui.add_enabled_ui(has_selection || has_area_selection, |ui| {
                    for duration in DurationChoice::ALL {
                        if ui
                            .selectable_label(self.selected_duration == duration, duration.label())
                            .clicked()
                        {
                            self.selected_duration = duration;
                            if has_selection {
                                self.apply_selected_duration();
                            } else {
                                self.apply_selected_duration_to_area_selection();
                            }
                            ui.close();
                        }
                    }
                });
            });
            submenu(ui, "Stretching", |ui| {
                ui.label("Step increment for dragging a note's left/right edge");
                ui.horizontal_wrapped(|ui| {
                    for (label, ticks) in [
                        ("1/32", TICKS_PER_QUARTER / 8),
                        ("1/16", TICKS_PER_QUARTER / 4),
                        ("1/8", TICKS_PER_QUARTER / 2),
                        ("1/4", TICKS_PER_QUARTER),
                        ("1/2", TICKS_PER_QUARTER * 2),
                        ("Whole", TICKS_PER_QUARTER * 4),
                    ] {
                        if ui
                            .selectable_label(self.stretch_step_ticks == ticks, label)
                            .clicked()
                        {
                            self.stretch_step_ticks = ticks;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Custom:");
                    ui.add(
                        egui::DragValue::new(&mut self.stretch_step_ticks)
                            .range(1..=TICKS_PER_QUARTER * 4)
                            .suffix(" ticks"),
                    );
                });
            });
            submenu(ui, "Dragging", |ui| {
                ui.label("Step increment for dragging a note (from its bar's start)");
                ui.horizontal_wrapped(|ui| {
                    for (label, ticks) in [
                        ("1/32", TICKS_PER_QUARTER / 8),
                        ("1/16", TICKS_PER_QUARTER / 4),
                        ("1/8", TICKS_PER_QUARTER / 2),
                        ("1/4", TICKS_PER_QUARTER),
                        ("1/2", TICKS_PER_QUARTER * 2),
                        ("Whole", TICKS_PER_QUARTER * 4),
                    ] {
                        if ui
                            .selectable_label(self.drag_step_ticks == ticks, label)
                            .clicked()
                        {
                            self.drag_step_ticks = ticks;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Custom:");
                    ui.add(
                        egui::DragValue::new(&mut self.drag_step_ticks)
                            .range(1..=TICKS_PER_QUARTER * 4)
                            .suffix(" ticks"),
                    );
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Hold Shift while dragging to disable snapping")
                        .small()
                        .color(egui::Color32::from_gray(140)),
                );
            });
            submenu(ui, "Dynamic", |ui| {
                ui.add_enabled_ui(has_selection, |ui| {
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
                            ui.close();
                        }
                    }
                });
            });
            ui.add_enabled_ui(has_selection, |ui| {
                note_effects_changed |= ui
                    .checkbox(&mut self.active_effects.ghost, "Ghost Note  (O)")
                    .changed();
                note_effects_changed |= ui
                    .checkbox(&mut self.active_effects.accent, "Accented Note  (;)")
                    .changed();
                note_effects_changed |= ui
                    .checkbox(
                        &mut self.active_effects.heavy_accent,
                        "Heavily Accented Note  (:)",
                    )
                    .changed();
                note_effects_changed |= ui
                    .checkbox(&mut self.active_effects.staccato, "Staccato  (!)")
                    .changed();
            });
            for label in [
                "Staccatissimo",
                "Tenuto",
                "Tie Note  (L)",
                "Tie Beat  (Shift+L)",
                "Rest  (R)",
                "Fermata...  (F)",
                "Accidentals",
            ] {
                planned_menu_item(ui, label);
            }
            ui.add_enabled_ui(has_selection, |ui| {
                if ui.button("One Semitone Down  (Alt+Shift+Down)").clicked() {
                    self.adjust_selected_fret(-1);
                    ui.close();
                }
                if ui.button("One Semitone Up  (Alt+Shift+Up)").clicked() {
                    self.adjust_selected_fret(1);
                    ui.close();
                }
            });
            for label in [
                "One Octave Down  (Alt+Shift+PgDown)",
                "One Octave Up  (Alt+Shift+PgUp)",
                "Left Hand Fingering...",
                "Right Hand Fingering...",
                "String Number",
                "Shift Down  (Alt+Down)",
                "Shift Up  (Alt+Up)",
                "Pickstroke Down  (Shift+D)",
                "Pickstroke Up  (Shift+U)",
                "Chord...  (A)",
                "Scale Diagram...  (Shift+S)",
                "Text...  (T)",
                "Timer  (@)",
                "Slash",
                "Barre...  (Shift+I)",
                "Octave Sign",
                "Design",
                "Audio Note Settings...  (Shift+F)",
            ] {
                planned_menu_item(ui, label);
            }
            if !has_selection {
                ui.label(egui::RichText::new("Select a note to edit it.").small());
            }
        });
        if note_effects_changed {
            self.apply_effects_to_selected();
        }
        submenu(ui, "Effects", |ui| {
            let mut effects_changed = false;
            ui.add_enabled_ui(has_selection, |ui| {
                planned_menu_item(ui, "Grace Note");
                planned_menu_item(ui, "Trill...  (N)");
                planned_menu_item(ui, "Ornament");
                planned_menu_item(ui, "Tremolo");
                effects_changed |= ui
                    .checkbox(&mut self.active_effects.let_ring, "Let Ring  (I)")
                    .changed();
                planned_menu_item(ui, "Sustain Pedal");
                planned_menu_item(ui, "Legato  (Shift+H)");
                effects_changed |= ui
                    .checkbox(&mut self.active_effects.hammer, "Hammer On / Pull Off  (H)")
                    .changed();
                planned_menu_item(ui, "Left Hand Tapping  (()");
                effects_changed |= ui
                    .checkbox(&mut self.active_effects.tapping, "Tapping  ())")
                    .changed();
                effects_changed |= ui
                    .checkbox(&mut self.active_effects.slapping, "Slap  (S)")
                    .changed();
                effects_changed |= ui
                    .checkbox(&mut self.active_effects.popping, "Pop  (Ctrl+S)")
                    .changed();
                planned_menu_item(ui, "Dead Slapped");
                effects_changed |= ui
                    .checkbox(&mut self.active_effects.dead, "Dead Note  (X)")
                    .changed();
                effects_changed |= ui
                    .checkbox(&mut self.active_effects.palm_mute, "Palm Mute on Note  (P)")
                    .changed();
                planned_menu_item(ui, "Palm Mute on Beat  (Shift+P)");
                planned_menu_item(ui, "Pick Scrape Out Downwards");
                planned_menu_item(ui, "Pick Scrape Out Upwards");
                planned_menu_item(ui, "Bend...  (B)");
                effects_changed |= ui
                    .checkbox(&mut self.active_effects.slide, "Slide")
                    .changed();
                planned_menu_item(ui, "Tremolo Bar...  (Shift+W)");
                effects_changed |= ui
                    .checkbox(&mut self.active_effects.vibrato, "Vibrato")
                    .changed();
                planned_menu_item(ui, "Vibrato w/ Trem. Bar");
                planned_menu_item(ui, "Natural Harmonic  (Y)");
                planned_menu_item(ui, "Artificial Harmonic...  (Ctrl+Alt+Y)");
                planned_menu_item(ui, "Brush Downstroke...  (Ctrl+D)");
                planned_menu_item(ui, "Brush Upstroke...  (Ctrl+U)");
                planned_menu_item(ui, "Arpeggio Down...  (Ctrl+Shift+D)");
                planned_menu_item(ui, "Arpeggio Up...  (Ctrl+Shift+U)");
                planned_menu_item(ui, "Rasgueado...  (Shift+R)");
                planned_menu_item(ui, "Golpe Finger");
                planned_menu_item(ui, "Golpe Thumb");
                effects_changed |= ui
                    .checkbox(&mut self.active_effects.fade_in, "Fade In  (<)")
                    .changed();
                planned_menu_item(ui, "Fade Out  (>)");
                planned_menu_item(ui, "Volume Swell  (Alt+<)");
                planned_menu_item(ui, "Wah Open  (Ctrl+Alt+O)");
                planned_menu_item(ui, "Wah Close  (Ctrl+Alt+C)");
            });
            if effects_changed {
                self.apply_effects_to_selected();
            }
        });
    }

    /// Lays a track's bars out into rows ("systems"): each system is a
    /// contiguous run of whole bars that fits within `available_width`. A
    /// bar is never split across two systems — if the next bar wouldn't fit
    /// in the remaining space of the current row, the whole row ends there
    /// and that bar starts the next one, mirroring how real notation
    /// software wraps.
    fn layout_staff_systems(&self, track: &InstrumentTrack, available_width: f32) -> Vec<StaffSystem> {
        let mut bars: Vec<(Tick, Tick)> = Vec::with_capacity(track.bar_count.max(1) as usize);
        let mut cursor: Tick = 0;
        for _ in 0..track.bar_count.max(1) {
            let (start, end) = track.measure_bounds_at(cursor);
            bars.push((start, end));
            cursor = end;
        }

        let min_bar_width = 24.0_f32;
        let mut systems: Vec<StaffSystem> = Vec::new();
        let mut current: Vec<(Tick, Tick)> = Vec::new();
        let mut current_width = 0.0_f32;
        for bar in bars {
            let bar_width = ((bar.1 - bar.0) as f32 * self.pixels_per_tick).max(min_bar_width);
            if !current.is_empty() && current_width + bar_width > available_width {
                systems.push(StaffSystem::from_bars(std::mem::take(&mut current)));
                current_width = 0.0;
            }
            current.push(bar);
            current_width += bar_width;
        }
        if !current.is_empty() {
            systems.push(StaffSystem::from_bars(current));
        }
        if systems.is_empty() {
            systems.push(StaffSystem {
                start_tick: 0,
                end_tick: 0,
                bars: Vec::new(),
            });
        }
        systems
    }

    /// Draws a "♩=bpm" label above the staff at each non-hidden tempo point
    /// falling within `[system_origin_tick, system_origin_tick+system_ticks)`,
    /// positioned right above the point's tick — matching Guitar Pro's
    /// automation display.
    fn paint_tempo_labels(
        &self,
        painter: &egui::Painter,
        left: f32,
        staff_y: f32,
        system_origin_tick: Tick,
        system_ticks: Tick,
    ) {
        for point in self.project.tempo_points.iter().filter(|point| {
            !point.hidden
                && point.at_tick >= system_origin_tick
                && point.at_tick < system_origin_tick + system_ticks
        }) {
            let x = left + (point.at_tick - system_origin_tick) as f32 * self.pixels_per_tick;
            painter.text(
                Pos2::new(x, staff_y - 12.0),
                Align2::LEFT_BOTTOM,
                format!("♩={}", point.bpm.round() as i32),
                FontId::proportional(12.0),
                self.theme.notation_foreground,
            );
        }
    }

    fn paint_selected_track_page(&self, painter: &egui::Painter, page: Rect) -> Rect {
        let Some(track) = self.project.track(self.selected_track_id) else {
            return page;
        };
        let left = page.left() + 54.0;
        let right = page.right() - 18.0;
        let available_width = (right - left).max(40.0);
        let systems = self.layout_staff_systems(track, available_width);
        let system_height = 168.0;
        let first_top = page.top() + 56.0;

        painter.text(
            Pos2::new(page.left() + 16.0, page.top() + 12.0),
            Align2::LEFT_TOP,
            format!("{} - {}", self.project.title, track.name),
            FontId::proportional(15.0),
            self.theme.notation_foreground,
        );

        for (system_index, system) in systems.iter().enumerate() {
            let top = first_top + system_index as f32 * system_height;
            if top + 136.0 > page.bottom() {
                break;
            }
            let staff_y = top;
            let tab_y = top + 82.0;
            let system_origin_tick = system.start_tick;
            let system_ticks = system.tick_span();
            // The row only extends as far as its actual bars reach — never
            // stretched out to the page edge, and never longer than the
            // page allows either (a single oversized bar still gets clipped
            // visually rather than overflowing the layout).
            let system_right = (left + system_ticks as f32 * self.pixels_per_tick).min(right);

            for line in 0..5 {
                let y = staff_y + line as f32 * 7.0;
                painter.line_segment(
                    [Pos2::new(left, y), Pos2::new(system_right, y)],
                    self.theme.staff_stroke(),
                );
            }
            for string in 0..track.tuning.len() {
                let y = tab_y + string as f32 * 9.0;
                painter.line_segment(
                    [Pos2::new(left, y), Pos2::new(system_right, y)],
                    self.theme.staff_stroke(),
                );
            }
            painter.text(
                Pos2::new(left - 26.0, staff_y + 14.0),
                Align2::CENTER_CENTER,
                "G",
                FontId::proportional(28.0),
                self.theme.notation_foreground,
            );

            self.paint_tempo_labels(painter, left, staff_y, system_origin_tick, system_ticks);

            for &(bar_start, _bar_end) in &system.bars {
                let x = left + (bar_start - system_origin_tick) as f32 * self.pixels_per_tick;
                painter.line_segment(
                    [Pos2::new(x, staff_y), Pos2::new(x, tab_y + 48.0)],
                    self.theme.bar_stroke(),
                );
                // Only label a bar where the signature actually *changes*
                // (or the very first bar of the piece) — not every bar that
                // happens to share the current signature.
                let sig = track.signature_at(bar_start);
                let is_change = bar_start == 0 || track.signature_at(bar_start - 1) != sig;
                if is_change {
                    painter.text(
                        Pos2::new(x + 6.0, staff_y + 14.0),
                        Align2::LEFT_CENTER,
                        format!("{}\n{}", sig.numerator, sig.denominator),
                        FontId::monospace(14.0),
                        self.theme.notation_foreground,
                    );
                }
            }
            // Closing wall at the end of the last bar in this row.
            painter.line_segment(
                [
                    Pos2::new(system_right, staff_y),
                    Pos2::new(system_right, tab_y + 48.0),
                ],
                self.theme.bar_stroke(),
            );

            self.paint_cursor_cell(painter, left, tab_y, system_origin_tick, system_ticks);

            let system_end_tick = system_origin_tick + system_ticks;

            // Continuation fragments first: notes whose onset was in an
            // earlier system/row but that still sustain into this one (this
            // app's stand-in for a tied note, until real ties exist — see
            // Plan.md). Drawn with no left border so they read as a
            // seamless continuation rather than a new note.
            for note in track.notes.iter().filter(|note| {
                note.abs_tick < system_origin_tick
                    && note.abs_tick + note.duration_ticks > system_origin_tick
            }) {
                let fragment_end = (note.abs_tick + note.duration_ticks).min(system_end_tick);
                self.paint_note_fragment(
                    painter,
                    left,
                    staff_y,
                    tab_y,
                    system_origin_tick,
                    system_right,
                    note,
                    system_origin_tick,
                    fragment_end,
                    false,
                );
            }

            // Starting fragments: notes whose onset falls in this system. If
            // a note's duration reaches past this row's right wall, only the
            // portion up to the wall is drawn here (no right border, since
            // it continues as a continuation fragment in a later system).
            for note in track.notes.iter().filter(|note| {
                note.abs_tick >= system_origin_tick && note.abs_tick < system_end_tick
            }) {
                let fragment_end = (note.abs_tick + note.duration_ticks).min(system_end_tick);
                self.paint_note_fragment(
                    painter,
                    left,
                    staff_y,
                    tab_y,
                    system_origin_tick,
                    system_right,
                    note,
                    note.abs_tick,
                    fragment_end,
                    true,
                );
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

    /// Draws one visual slice of a note. A note whose duration reaches past
    /// its row's right wall is drawn as multiple fragments across rows: the
    /// first (`is_onset`) carries the standard-notation dot/stem, the fret
    /// digit, and any effect labels; later continuation fragments are a bare
    /// block with no left border, so the note reads as one continuous sound
    /// "picking back up" at the start of the next row rather than a second,
    /// separate note. `fragment_start`/`fragment_end` are already clipped to
    /// this row's `[system_origin_tick, system_origin_tick+system_ticks)`.
    #[allow(clippy::too_many_arguments)]
    fn paint_note_fragment(
        &self,
        painter: &egui::Painter,
        left: f32,
        staff_y: f32,
        tab_y: f32,
        system_origin_tick: Tick,
        system_right: f32,
        note: &crate::core::project::TabNote,
        fragment_start: Tick,
        fragment_end: Tick,
        is_onset: bool,
    ) {
        let note_end = note.abs_tick + note.duration_ticks;
        let x = left + (fragment_start - system_origin_tick) as f32 * self.pixels_per_tick;
        let tab_line_y = tab_y + note.string_index as f32 * 9.0;
        let selected = self.selected_note_id == Some(note.id)
            || self.area_selected_note_ids.contains(&note.id);

        if is_onset {
            let staff_note_y = staff_y + 28.0 - (note.fret as f32 % 12.0) * 1.7;
            painter.circle_filled(
                Pos2::new(x + 8.0, staff_note_y),
                4.2,
                if selected {
                    self.theme.accent_color
                } else {
                    self.theme.notation_foreground
                },
            );
            painter.line_segment(
                [
                    Pos2::new(x + 13.0, staff_note_y),
                    Pos2::new(x + 13.0, staff_note_y - 24.0),
                ],
                Stroke::new(1.4, self.theme.notation_foreground),
            );
        }

        // Fragment width is strictly proportional to however much of the
        // note's duration falls in this row. The onset fragment is floored
        // wide enough to keep the fret digit legible (two-digit frets need a
        // touch more room); continuation fragments carry no digit, so no
        // floor is needed. Either way it's capped at the row's right wall —
        // never poking past it, even in a cramped corner case.
        let proportional = (fragment_end - fragment_start).max(0) as f32 * self.pixels_per_tick;
        let min_for_digits = if !is_onset {
            0.0
        } else if note.fret >= 10 {
            13.0
        } else {
            8.0
        };
        let width = proportional
            .max(min_for_digits)
            .min((system_right - x).max(2.0));
        // Slightly shorter bricks for very brief notes so the visual
        // rhythm matches the musical one, without squashing legible ones.
        let height = if proportional < 10.0 { 9.0 } else { 12.0 };
        let note_rect = Rect::from_center_size(
            Pos2::new(x + width * 0.5, tab_line_y),
            Vec2::new(width, height),
        );

        // The border is open on whichever side(s) this fragment continues
        // through, so consecutive fragments of the same note read as one
        // unbroken block rather than two adjacent ones.
        paint_note_block(
            painter,
            note_rect,
            if selected {
                self.theme.accent_color
            } else {
                self.theme.note_fill
            },
            self.theme.note_stroke(),
            !is_onset,
            fragment_end < note_end,
        );

        if is_onset {
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

    /// Finds the note (if any) whose visible fragment — onset or
    /// continuation, in whichever row `pos` falls in — contains `pos`.
    /// Shared by click-to-select and drag-start (which uses it to decide
    /// whether a drag begins a rubber-band area-selection or just grabs an
    /// existing note).
    fn find_note_fragment_at(&self, pos: Pos2, page: Rect) -> Option<u64> {
        if !page.contains(pos) {
            return None;
        }
        let track = self.project.track(self.selected_track_id)?;
        let left = page.left() + 54.0;
        let right = page.right() - 18.0;
        let available_width = (right - left).max(40.0);
        let systems = self.layout_staff_systems(track, available_width);
        let system_height = 168.0;
        let first_top = page.top() + 56.0;
        let system_index = (((pos.y - first_top) / system_height).floor().max(0.0) as usize)
            .min(systems.len().saturating_sub(1));
        let system = systems.get(system_index)?;
        let system_origin_tick = system.start_tick;
        let system_end_tick = system_origin_tick + system.tick_span();
        let tab_y = first_top + system_index as f32 * system_height + 82.0;
        let string_gap = 9.0;

        // A note's *onset* (abs_tick falls in this system) as well as a
        // *continuation* fragment (the note started in an earlier row but
        // still sustains into this one; see `paint_note_fragment`) both
        // count as hits — without the continuation case, clicking the
        // "teleported" half of a split note wouldn't select it, letting a
        // new note get typed right on top of it instead of onto empty space.
        track.notes.iter().find_map(|note| {
            let note_end = note.abs_tick + note.duration_ticks;
            if note_end <= system_origin_tick || note.abs_tick >= system_end_tick {
                return None;
            }
            let fragment_start = note.abs_tick.max(system_origin_tick);
            let fragment_end = note_end.min(system_end_tick);
            let x = left + (fragment_start - system_origin_tick) as f32 * self.pixels_per_tick;
            let width = ((fragment_end - fragment_start) as f32 * self.pixels_per_tick).max(22.0);
            let y = tab_y + note.string_index as f32 * string_gap;
            let rect =
                Rect::from_center_size(Pos2::new(x + width * 0.5, y), Vec2::new(width, 14.0));
            rect.contains(pos).then_some(note.id)
        })
    }

    /// Every note on the selected track with a fragment (onset or
    /// continuation, in any visible row) intersecting `selection_rect`.
    /// Used by the canvas's rubber-band area-selection.
    fn notes_intersecting_rect(&self, selection_rect: Rect, page: Rect) -> Vec<u64> {
        let Some(track) = self.project.track(self.selected_track_id) else {
            return Vec::new();
        };
        let left = page.left() + 54.0;
        let right = page.right() - 18.0;
        let available_width = (right - left).max(40.0);
        let systems = self.layout_staff_systems(track, available_width);
        let system_height = 168.0;
        let first_top = page.top() + 56.0;
        let string_gap = 9.0;

        let mut hits = Vec::new();
        for (system_index, system) in systems.iter().enumerate() {
            let top = first_top + system_index as f32 * system_height;
            if top + 136.0 > page.bottom() {
                break;
            }
            let tab_y = top + 82.0;
            let system_origin_tick = system.start_tick;
            let system_end_tick = system_origin_tick + system.tick_span();

            for note in &track.notes {
                let note_end = note.abs_tick + note.duration_ticks;
                if note_end <= system_origin_tick || note.abs_tick >= system_end_tick {
                    continue;
                }
                let fragment_start = note.abs_tick.max(system_origin_tick);
                let fragment_end = note_end.min(system_end_tick);
                let x = left + (fragment_start - system_origin_tick) as f32 * self.pixels_per_tick;
                let width =
                    ((fragment_end - fragment_start) as f32 * self.pixels_per_tick).max(22.0);
                let y = tab_y + note.string_index as f32 * string_gap;
                let note_rect =
                    Rect::from_center_size(Pos2::new(x + width * 0.5, y), Vec2::new(width, 14.0));
                if selection_rect.intersects(note_rect) && !hits.contains(&note.id) {
                    hits.push(note.id);
                }
            }
        }
        hits
    }

    /// Selects a single note (clearing any area selection) and syncs the
    /// toolbar's active velocity/effects and the keyboard cursor to it, so
    /// subsequent typing edits it in place.
    fn select_single_note(&mut self, note_id: u64) {
        self.selected_note_id = Some(note_id);
        self.area_selected_note_ids.clear();
        if let Some(track) = self.project.track(self.selected_track_id)
            && let Some(note) = track.notes.iter().find(|note| note.id == note_id)
        {
            self.active_velocity = note.velocity;
            self.active_effects = note.effects.clone();
            self.cursor.tick = note.abs_tick;
            self.cursor.string_index = note.string_index;
            self.cursor.pending_fret = None;
            self.cursor.pending_at = None;
        }
        self.status = format!("Selected note {note_id}.");
    }

    /// Deletes every note in `area_selected_note_ids` as a single undo step.
    fn delete_area_selected_notes(&mut self) {
        if self.area_selected_note_ids.is_empty() {
            return;
        }
        let track_id = self.selected_track_id;
        let before = self.project.clone();
        let ids: std::collections::HashSet<u64> =
            self.area_selected_note_ids.iter().copied().collect();
        let Some(track) = self.project.track_mut(track_id) else {
            return;
        };
        let removed_count_before = track.notes.len();
        track.notes.retain(|note| !ids.contains(&note.id));
        let removed = removed_count_before - track.notes.len();
        if removed > 0 {
            self.record_project_edit(before);
            self.refresh_after_edit(track_id);
            self.status = format!("Deleted {removed} notes.");
        }
        self.area_selected_note_ids.clear();
    }

    /// Called on `Response::drag_started()` for the staff canvas: starting a
    /// drag right on a note just selects it (like a click); starting it on
    /// empty space begins a rubber-band area-selection.
    /// If `pos` is near the true left or right edge of the currently
    /// *selected* note (within a few pixels, in whichever row that edge
    /// falls in), returns which edge. Only a note's actual start/end tick
    /// count as a grabbable edge — not the open side of a continuation
    /// fragment where it merely wraps into a new row (see
    /// `paint_note_fragment`); there's no real boundary there to drag.
    ///
    /// Checks every note in the current selection (single click-select or
    /// multi area-select, whichever is active) and returns the first one
    /// whose edge `pos` is near, along with which edge — grabbing *any*
    /// selected note's edge is enough to start stretching the whole
    /// selection together (see `begin_canvas_drag`).
    fn stretch_edge_at(&self, pos: Pos2, page: Rect) -> Option<(u64, StretchEdge)> {
        // Generous on purpose: this is also what decides where the resize
        // cursor icon shows up, so the hit area should feel at least as big
        // as the visible resize-cursor region, not just a couple of pixels
        // right at the drawn border.
        const EDGE_GRAB_PIXELS: f32 = 10.0;

        let candidate_ids: &[u64] = if let Some(id) = self.selected_note_id.as_ref() {
            std::slice::from_ref(id)
        } else {
            &self.area_selected_note_ids
        };
        if candidate_ids.is_empty() {
            return None;
        }
        let track = self.project.track(self.selected_track_id)?;

        let left = page.left() + 54.0;
        let right = page.right() - 18.0;
        let available_width = (right - left).max(40.0);
        let systems = self.layout_staff_systems(track, available_width);
        let system_height = 168.0;
        let first_top = page.top() + 56.0;
        let system_index = (((pos.y - first_top) / system_height).floor().max(0.0) as usize)
            .min(systems.len().saturating_sub(1));
        let system = systems.get(system_index)?;
        let system_origin_tick = system.start_tick;
        let system_end_tick = system_origin_tick + system.tick_span();

        for &note_id in candidate_ids {
            let Some(note) = track.notes.iter().find(|note| note.id == note_id) else {
                continue;
            };
            let note_end = note.abs_tick + note.duration_ticks;
            if note.abs_tick >= system_origin_tick && note.abs_tick < system_end_tick {
                let x = left + (note.abs_tick - system_origin_tick) as f32 * self.pixels_per_tick;
                if (pos.x - x).abs() <= EDGE_GRAB_PIXELS {
                    return Some((note_id, StretchEdge::Left));
                }
            }
            if note_end > system_origin_tick && note_end <= system_end_tick {
                let x = left + (note_end - system_origin_tick) as f32 * self.pixels_per_tick;
                if (pos.x - x).abs() <= EDGE_GRAB_PIXELS {
                    return Some((note_id, StretchEdge::Right));
                }
            }
        }
        None
    }

    /// Computes the actual (possibly snapped) tick offset that a note-move
    /// drag would apply right now, given the anchor note's actual position
    /// and how far the raw drag has moved. Shared by the live preview and
    /// the real move applied on release, so they can never disagree.
    fn compute_move_snap_offset(
        &self,
        anchor_note_id: u64,
        raw_delta_ticks: f64,
        snapping_disabled: bool,
    ) -> Tick {
        if snapping_disabled {
            return raw_delta_ticks.round() as Tick;
        }
        let Some(track) = self.project.track(self.selected_track_id) else {
            return raw_delta_ticks.round() as Tick;
        };
        let Some(anchor) = track.notes.iter().find(|note| note.id == anchor_note_id) else {
            return raw_delta_ticks.round() as Tick;
        };
        let anchor_original_tick = anchor.abs_tick;
        let anchor_raw_tick = anchor_original_tick as f64 + raw_delta_ticks;

        // Snapping is relative to the *bar* the grabbed note started in,
        // not the project's tick 0 — dragging within a bar should feel
        // anchored to that bar's own grid.
        let (bar_start, _) = track.measure_bounds_at(anchor_original_tick);
        let grid = self.drag_step_ticks.max(1) as f64;
        let offset_from_bar_start = anchor_raw_tick - bar_start as f64;
        let nearest_grid_tick = bar_start as f64 + (offset_from_bar_start / grid).round() * grid;
        // Elastic snap: only pull to the grid line when close to it, and
        // recomputed fresh each call from the fixed original position — a
        // small nudge can never compound into a runaway jump, and dragging
        // far enough away from a grid line "unsnaps" back to the raw
        // position under the cursor.
        const SNAP_THRESHOLD_PIXELS: f64 = 6.0;
        let snap_threshold_ticks = SNAP_THRESHOLD_PIXELS / self.pixels_per_tick as f64;
        let snapped_anchor_tick =
            if (anchor_raw_tick - nearest_grid_tick).abs() <= snap_threshold_ticks {
                nearest_grid_tick
            } else {
                anchor_raw_tick.round()
            };
        (snapped_anchor_tick - anchor_original_tick as f64) as Tick
    }

    fn begin_canvas_drag(&mut self, pos: Pos2, page: Rect) {
        if let Some((grabbed_note_id, edge)) = self.stretch_edge_at(pos, page) {
            // Grabbing any selected note's edge stretches the whole
            // selection together; grabbing an unselected note's edge (only
            // possible via `self.selected_note_id`, since `stretch_edge_at`
            // only checks the current selection) stretches just it.
            let note_ids = if self.area_selected_note_ids.contains(&grabbed_note_id) {
                self.area_selected_note_ids.clone()
            } else {
                vec![grabbed_note_id]
            };
            self.canvas_drag = CanvasDragMode::Stretching {
                note_ids,
                edge,
                start_pos: pos,
                current_pos: pos,
            };
        } else if let Some(note_id) = self.find_note_fragment_at(pos, page) {
            let already_selected = self.selected_note_id == Some(note_id)
                || self.area_selected_note_ids.contains(&note_id);
            if already_selected {
                // Dragging the body of an already-selected note moves it
                // (and the rest of the selection, if any) instead of
                // re-selecting or starting an area-select.
                let note_ids = if self.area_selected_note_ids.contains(&note_id) {
                    self.area_selected_note_ids.clone()
                } else {
                    vec![note_id]
                };
                self.canvas_drag = CanvasDragMode::MovingNotes {
                    note_ids,
                    anchor_note_id: note_id,
                    start_pos: pos,
                    current_pos: pos,
                };
            } else {
                self.select_single_note(note_id);
                self.canvas_drag = CanvasDragMode::Idle;
            }
        } else {
            self.canvas_drag = CanvasDragMode::AreaSelecting {
                start_pos: pos,
                current_pos: pos,
            };
        }
    }

    fn update_canvas_drag(&mut self, pos: Pos2) {
        match &mut self.canvas_drag {
            CanvasDragMode::AreaSelecting { current_pos, .. } => {
                *current_pos = pos;
            }
            CanvasDragMode::Stretching { current_pos, .. } => {
                *current_pos = pos;
            }
            CanvasDragMode::MovingNotes { current_pos, .. } => {
                *current_pos = pos;
            }
            CanvasDragMode::Idle => {}
        }
    }

    fn finish_canvas_drag(&mut self, page: Rect, snapping_disabled: bool) {
        match &self.canvas_drag {
            CanvasDragMode::AreaSelecting {
                start_pos,
                current_pos,
            } => {
                let selection_rect = Rect::from_two_pos(*start_pos, *current_pos);
                self.area_selected_note_ids = self.notes_intersecting_rect(selection_rect, page);
                self.selected_note_id = None;
                if !self.area_selected_note_ids.is_empty() {
                    self.status =
                        format!("{} notes selected.", self.area_selected_note_ids.len());
                }
            }
            CanvasDragMode::Stretching {
                note_ids,
                edge,
                start_pos,
                current_pos,
            } => {
                let delta_ticks =
                    ((current_pos.x - start_pos.x) / self.pixels_per_tick).round() as Tick;
                let track_id = self.selected_track_id;
                let step = self.stretch_step_ticks;
                let edge = *edge;
                let note_ids = note_ids.clone();
                let previous_project = self.project.clone();
                let mut stretched_count = 0;
                for note_id in &note_ids {
                    if self
                        .project
                        .stretch_note(track_id, *note_id, edge, delta_ticks, step)
                    {
                        stretched_count += 1;
                    }
                }
                if stretched_count > 0 {
                    self.record_project_edit(previous_project);
                    self.refresh_after_edit(track_id);
                    let edge_label = match edge {
                        StretchEdge::Left => "left",
                        StretchEdge::Right => "right",
                    };
                    self.status = if note_ids.len() > 1 {
                        format!("Stretched the {edge_label} edge of {stretched_count} notes.")
                    } else {
                        format!("Stretched the {edge_label} edge of note {}.", note_ids[0])
                    };
                }
            }
            CanvasDragMode::MovingNotes {
                note_ids,
                anchor_note_id,
                start_pos,
                current_pos,
            } => {
                let track_id = self.selected_track_id;
                let note_ids = note_ids.clone();
                let anchor_note_id = *anchor_note_id;
                let raw_delta_ticks =
                    (current_pos.x - start_pos.x) as f64 / self.pixels_per_tick as f64;
                let effective_offset_ticks = self.compute_move_snap_offset(
                    anchor_note_id,
                    raw_delta_ticks,
                    snapping_disabled,
                );
                // Each string is already a discrete unit, so vertical
                // movement just rounds to the nearest one — no snap grid
                // or threshold needed the way the tick axis has.
                const STRING_GAP: f32 = 9.0;
                let string_delta = ((current_pos.y - start_pos.y) / STRING_GAP).round() as i32;

                if effective_offset_ticks != 0 || string_delta != 0 {
                    let previous_project = self.project.clone();
                    let mut moved_count = 0;
                    for note_id in &note_ids {
                        if self.project.move_note(
                            track_id,
                            *note_id,
                            effective_offset_ticks,
                            string_delta,
                            &note_ids,
                        ) {
                            moved_count += 1;
                        }
                    }
                    if moved_count > 0 {
                        self.record_project_edit(previous_project);
                        self.refresh_after_edit(track_id);
                        self.status = if note_ids.len() > 1 {
                            format!("Moved {moved_count} notes.")
                        } else {
                            format!("Moved note {}.", note_ids[0])
                        };
                    }
                }
            }
            CanvasDragMode::Idle => {}
        }
        self.canvas_drag = CanvasDragMode::Idle;
    }

    fn handle_canvas_click(&mut self, pos: Pos2, page: Rect) {
        if !page.contains(pos) {
            return;
        }

        // Checked before borrowing `track` below (rather than interleaved
        // with it) to keep this immune to any borrow-checker ambiguity
        // between the `&self` hit test and the `&mut self` select call.
        if let Some(note_id) = self.find_note_fragment_at(pos, page) {
            self.select_single_note(note_id);
            return;
        }

        let track_id = self.selected_track_id;
        let Some(track) = self.project.track(track_id) else {
            return;
        };
        let left = page.left() + 54.0;
        let right = page.right() - 18.0;
        let available_width = (right - left).max(40.0);
        let systems = self.layout_staff_systems(track, available_width);
        let system_height = 168.0;
        let first_top = page.top() + 56.0;
        let system_index = (((pos.y - first_top) / system_height).floor().max(0.0) as usize)
            .min(systems.len().saturating_sub(1));
        let Some(system) = systems.get(system_index) else {
            return;
        };
        let system_origin_tick = system.start_tick;
        let clicked_tick = ((((pos.x - left) / self.pixels_per_tick).max(0.0)) as Tick
            + system_origin_tick)
            .min(system.end_tick.max(system_origin_tick));
        let snapped_tick = snap_tick(clicked_tick, TICKS_PER_QUARTER / 4);
        let tab_y = first_top + system_index as f32 * system_height + 82.0;
        let string_gap = 9.0;

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
        self.area_selected_note_ids.clear();
        self.status = format!(
            "Cursor at tick {} on string {}. Type a fret and press Enter.",
            snapped_tick,
            string_index + 1
        );
    }
}

/// Draws a note block's fill and border, leaving the border open on
/// whichever side(s) it continues through to an adjacent fragment of the
/// same note (see `BetterWriterApp::paint_note_fragment`). `egui::Painter`
/// has no built-in "stroke three sides of a rect" primitive, so this fills
/// normally and then draws the top/bottom/left/right edges individually.
fn paint_note_block(
    painter: &egui::Painter,
    rect: Rect,
    fill: egui::Color32,
    stroke: Stroke,
    open_left: bool,
    open_right: bool,
) {
    painter.rect_filled(rect, 1.0, fill);
    painter.line_segment([rect.left_top(), rect.right_top()], stroke);
    painter.line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
    if !open_left {
        painter.line_segment([rect.left_top(), rect.left_bottom()], stroke);
    }
    if !open_right {
        painter.line_segment([rect.right_top(), rect.right_bottom()], stroke);
    }
}

fn snap_tick(tick: Tick, grid: Tick) -> Tick {
    if grid <= 0 {
        tick
    } else {
        (tick / grid) * grid
    }
}

/// The rightmost tick the editor needs to render for a project: as many bars
/// as each track explicitly declares (`bar_count`), or further out if a note
/// somehow sits past that (e.g. an old save prior to explicit bar counts).
fn horizon_tick(project: &BwxProject) -> Tick {
    project
        .tracks
        .iter()
        .map(|track| {
            let bars_end = track.bars_end_tick();
            let notes_end = track
                .notes
                .iter()
                .map(|note| note.end_tick())
                .max()
                .unwrap_or(0);
            bars_end.max(notes_end)
        })
        .max()
        .unwrap_or(TICKS_PER_QUARTER * 4)
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

/// Where the recent-projects list is persisted: a small plain-text file
/// (one path per line) next to the running executable. Avoids pulling in a
/// platform-config-dir dependency for what's a short, disposable list.
fn recent_projects_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|dir| dir.join("betterwriter_recent.txt"))
}

fn load_recent_projects() -> Vec<PathBuf> {
    let Some(path) = recent_projects_path() else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .take(MAX_RECENT_PROJECTS)
        .collect()
}

fn save_recent_projects(paths: &[PathBuf]) {
    let Some(path) = recent_projects_path() else {
        return;
    };
    let content = paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let _ = std::fs::write(path, content);
}

/// Picks a name that doesn't collide (case-insensitively) with any recent
/// project's file stem: `base`, then `base 1`, `base 2`, ... Falls back to
/// "Untitled" if `base` is blank.
fn unique_project_name(base: &str, recent: &[PathBuf]) -> String {
    let base = {
        let trimmed = base.trim();
        if trimmed.is_empty() { "Untitled" } else { trimmed }
    };
    let existing: std::collections::HashSet<String> = recent
        .iter()
        .filter_map(|path| path.file_stem())
        .filter_map(|stem| stem.to_str())
        .map(str::to_ascii_lowercase)
        .collect();

    if !existing.contains(&base.to_ascii_lowercase()) {
        return base.to_owned();
    }
    let mut suffix = 1u32;
    loop {
        let candidate = format!("{base} {suffix}");
        if !existing.contains(&candidate.to_ascii_lowercase()) {
            return candidate;
        }
        suffix += 1;
    }
}

/// Scores how well `query` matches `target` for the Start Menu's recent-
/// projects search, as `(ordered_matches, overall_matches)`:
/// - `ordered_matches`: length of the longest run of query characters found
///   in `target`, in the same order, via a greedy subsequence scan (a classic
///   fuzzy-finder score).
/// - `overall_matches`: how many query characters appear in `target` at all,
///   regardless of order (each target character consumed at most once).
///
/// Sorting by this tuple descending (Rust compares tuples lexicographically)
/// naturally gives `ordered_matches` priority over `overall_matches`, per the
/// requested "matched in order, then overall" ranking.
fn search_match_score(query: &str, target: &str) -> (usize, usize) {
    let query_chars: Vec<char> = query.to_lowercase().chars().collect();
    let target_chars: Vec<char> = target.to_lowercase().chars().collect();

    let mut ordered_matches = 0usize;
    let mut target_cursor = 0usize;
    for &query_char in &query_chars {
        while target_cursor < target_chars.len() && target_chars[target_cursor] != query_char {
            target_cursor += 1;
        }
        if target_cursor < target_chars.len() {
            ordered_matches += 1;
            target_cursor += 1;
        }
    }

    let mut remaining_counts: std::collections::HashMap<char, usize> =
        std::collections::HashMap::new();
    for &target_char in &target_chars {
        *remaining_counts.entry(target_char).or_insert(0) += 1;
    }
    let mut overall_matches = 0usize;
    for &query_char in &query_chars {
        if let Some(count) = remaining_counts.get_mut(&query_char) {
            if *count > 0 {
                *count -= 1;
                overall_matches += 1;
            }
        }
    }

    (ordered_matches, overall_matches)
}

/// A submenu button that — unlike plain `ui.menu_button` — doesn't close the
/// whole context-menu stack when clicking a widget *inside* it, only when
/// clicking outside it. egui's menus default to
/// `PopupCloseBehavior::CloseOnClick` (close on *any* click, even inside the
/// menu's own body), which made interactive controls nested in a submenu
/// (e.g. the Stretching submenu's DragValue) effectively unusable — the
/// menu would vanish the instant you touched them. Items that *should*
/// still close on click (most actions) do so via their own `ui.close()`
/// call, same as before; this only changes what happens when there's no
/// such explicit call.
fn submenu<R>(ui: &mut egui::Ui, label: &str, add_contents: impl FnOnce(&mut egui::Ui) -> R) {
    egui::containers::menu::MenuButton::new(label)
        .config(
            egui::containers::menu::MenuConfig::default()
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
        )
        .ui(ui, add_contents);
}

/// A `DragValue` that scrolls through a fixed list of values (by index)
/// instead of a raw numeric range — the same drag-to-change / click-and-
/// scroll interaction as a normal `DragValue`, but each step moves to the
/// next *entry in the list* rather than the next integer. Used anywhere a
/// value is picked from a short set of musically-meaningful options (time
/// signature denominators, note durations) so cycling through them is one
/// smooth gesture instead of opening a dropdown. Returns the newly-picked
/// value if it changed this frame.
fn list_drag_value<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id_salt: &str,
    values: &[T],
    labels: &[&str],
    current: T,
) -> Option<T> {
    if values.is_empty() {
        return None;
    }
    let mut index = values.iter().position(|v| *v == current).unwrap_or(0) as i32;
    let max_index = values.len() as i32 - 1;
    let changed = ui
        .push_id(id_salt, |ui| {
            ui.add(
                egui::DragValue::new(&mut index)
                    .range(0..=max_index)
                    .custom_formatter(move |n, _| {
                        labels.get(n as usize).map(|s| s.to_string()).unwrap_or_default()
                    }),
            )
        })
        .inner
        .changed();
    changed.then(|| values[index.clamp(0, max_index) as usize])
}

fn small_tool_button(ui: &mut egui::Ui, label: &str, tooltip: &str) -> egui::Response {
    ui.add_sized([42.0, 22.0], egui::Button::new(label))
        .on_hover_text(tooltip)
}

/// Render a roadmap item exactly where its eventual command belongs, while
/// keeping it visibly unavailable until the underlying score operation exists.
fn planned_menu_item(ui: &mut egui::Ui, label: &str) {
    ui.add_enabled(false, egui::Button::new(label));
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