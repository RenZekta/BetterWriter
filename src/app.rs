use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::{
    audio::{AudioRuntime, Vst3HostSlot, VstHost},
    core::{
        BwxProject, CompatibilityReport, DurationChoice, InstrumentFamily, InstrumentTrack,
        MidiEventKind, MidiPlaybackEvent, NoteClip, NoteEffects, RestFill, ShadowTimeline,
        StandardExportFormat, StretchEdge, StringedVariant, TICKS_PER_QUARTER, TempoTransition,
        Tick, TimeSignature,
        sounds::{sound_choices, sound_name},
    },
    format,
    fretboard::{self, FretboardFrame, Inlay},
    icons::{self, PaletteIcon},
    notation::{self, Clef},
    theme::EditorTheme,
};

/// The keyboard-driven write cursor. It points at a `(tick, string)` cell on
/// the active track's tab staff - exactly the cell the next typed fret will
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
    /// Beat-column layout of each bar. Empty in brick view, where x is
    /// simply proportional to time.
    beat_bars: Vec<BeatBarLayout>,
}

/// Horizontal layout of one bar in tab view: its notes and rests sit in
/// columns whose spacing depends on their durations, not on a fixed
/// pixels-per-tick scale.
struct BeatBarLayout {
    start: Tick,
    end: Tick,
    /// Left edge of the bar, relative to the system's left edge.
    x: f32,
    width: f32,
    /// `(onset tick, center x)` of every beat in the bar, x relative to the
    /// system's left edge.
    columns: Vec<(Tick, f32)>,
}

impl StaffSystem {
    fn from_bars(bars: Vec<(Tick, Tick)>) -> Self {
        let start_tick = bars.first().map(|bar| bar.0).unwrap_or(0);
        let end_tick = bars.last().map(|bar| bar.1).unwrap_or(start_tick);
        Self {
            start_tick,
            end_tick,
            bars,
            beat_bars: Vec::new(),
        }
    }

    fn tick_span(&self) -> Tick {
        (self.end_tick - self.start_tick).max(0)
    }

    fn has_beat_layout(&self) -> bool {
        !self.beat_bars.is_empty()
    }

    /// X of `tick` relative to the system's left edge. In beat layout, ticks
    /// between two beats are interpolated within their bar.
    fn x_of(&self, tick: Tick, pixels_per_tick: f32) -> f32 {
        if !self.has_beat_layout() {
            return (tick - self.start_tick) as f32 * pixels_per_tick;
        }
        let bar = self
            .beat_bars
            .iter()
            .find(|bar| tick >= bar.start && tick < bar.end)
            .or(self.beat_bars.last());
        let Some(bar) = bar else {
            return 0.0;
        };
        let (Some(first), Some(last)) = (bar.columns.first(), bar.columns.last()) else {
            return bar.x;
        };
        if tick <= first.0 {
            return first.1;
        }
        if tick >= bar.end {
            return bar.x + bar.width - 2.0;
        }
        if tick >= last.0 {
            let span = (bar.end - last.0).max(1) as f32;
            let fraction = (tick - last.0) as f32 / span;
            return last.1 + (bar.x + bar.width - 2.0 - last.1) * fraction;
        }
        let next = bar.columns.partition_point(|column| column.0 <= tick);
        let (t0, x0) = bar.columns[next - 1];
        let (t1, x1) = bar.columns[next];
        x0 + (x1 - x0) * (tick - t0) as f32 / (t1 - t0).max(1) as f32
    }

    /// Left edge of bar `index` relative to the system's left edge.
    fn bar_x(&self, index: usize, pixels_per_tick: f32) -> f32 {
        match self.beat_bars.get(index) {
            Some(bar) => bar.x,
            None => self
                .bars
                .get(index)
                .map(|bar| (bar.0 - self.start_tick) as f32 * pixels_per_tick)
                .unwrap_or(0.0),
        }
    }

    /// Right edge of the system relative to its left edge.
    fn right_x(&self, pixels_per_tick: f32) -> f32 {
        match self.beat_bars.last() {
            Some(bar) => bar.x + bar.width,
            None => self.tick_span() as f32 * pixels_per_tick,
        }
    }

    /// The tick under `x` (relative to the system's left edge): the nearest
    /// beat in beat layout, the plain proportional position otherwise.
    fn tick_at(&self, x: f32, pixels_per_tick: f32) -> Tick {
        if !self.has_beat_layout() {
            return (x / pixels_per_tick).max(0.0) as Tick + self.start_tick;
        }
        let bar = self
            .beat_bars
            .iter()
            .find(|bar| x < bar.x + bar.width)
            .or(self.beat_bars.last());
        bar.and_then(|bar| {
            bar.columns
                .iter()
                .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
                .map(|column| column.0)
        })
        .unwrap_or(self.start_tick)
    }
}

/// How notes are drawn and edited on the staff canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorViewMode {
    /// Notes are bricks whose width is proportional to their duration. A
    /// sustained note is a single brick, even across barlines.
    Bricks,
    /// Guitar Pro style: bare fret numbers on the strings, every note inside
    /// one bar, with notes that cross a barline written as tied notes.
    TabNumbers,
}

/// Score zoom limits. They match the toolbar slider and the clamp that tab
/// view applies to its beat widths.
const MIN_PIXELS_PER_TICK: f32 = 0.035;
const MAX_PIXELS_PER_TICK: f32 = 0.14;
/// Zoom factor applied per scrolled point of mouse wheel (about 8% per notch).
const WHEEL_ZOOM_PER_POINT: f32 = 0.002;
/// Distance from the page's top edge to the first staff system.
const PAGE_TOP_INSET: f32 = 56.0;
const PAGE_LEFT_INSET: f32 = 54.0;
const PAGE_RIGHT_INSET: f32 = 18.0;
const PAGE_BOTTOM_PADDING: f32 = 24.0;

/// Placement of the staff systems on the page. Painting and every hit test
/// derive their rows from this one value, so the scroll offset applies
/// everywhere at once.
struct PageGeometry {
    left: f32,
    right: f32,
    first_top: f32,
    systems: Vec<StaffSystem>,
    metrics: SystemMetrics,
}

impl PageGeometry {
    fn system_top(&self, index: usize) -> f32 {
        self.first_top + index as f32 * self.metrics.height
    }

    /// The row at screen height `y`; positions above or below the rows clamp
    /// to the first or last one.
    fn system_index_at(&self, y: f32) -> usize {
        (((y - self.first_top) / self.metrics.height).floor().max(0.0) as usize)
            .min(self.systems.len().saturating_sub(1))
    }

    /// The row that contains `tick`, or the last row that starts before it.
    fn system_index_of_tick(&self, tick: Tick) -> usize {
        self.systems
            .iter()
            .rposition(|system| system.start_tick <= tick)
            .unwrap_or(0)
    }
}

/// Vertical layout of one staff system, which depends on whether the classic
/// notation staff is shown above the tablature and on the string count.
#[derive(Clone, Copy, Debug)]
struct SystemMetrics {
    /// Distance between the tops of consecutive systems.
    height: f32,
    /// Distance from a system's top to the top line of the classic staff.
    staff_offset: f32,
    /// Distance from a system's top to its first tab string.
    tab_offset: f32,
    string_count: usize,
}

impl SystemMetrics {
    const STRING_GAP: f32 = 9.0;
    /// Gap between adjacent classic staff lines.
    const STAFF_LINE_GAP: f32 = 7.0;
    /// Height of the five-line staff.
    const STAFF_HEIGHT: f32 = Self::STAFF_LINE_GAP * 4.0;
    /// Room above the staff for high notes' ledger lines, stems and labels.
    const STAFF_TOP_MARGIN: f32 = 36.0;
    /// Room between the staff and the tab for low notes' ledger lines and
    /// stems.
    const STAFF_BOTTOM_MARGIN: f32 = 40.0;
    /// Gap between a system's last tab string and its bottom edge.
    const BOTTOM_PADDING: f32 = 41.0;

    /// Distance from the first to the last tab string.
    fn tab_span(&self) -> f32 {
        self.string_count.saturating_sub(1) as f32 * Self::STRING_GAP
    }

    fn new(show_classic_notation: bool, string_count: usize) -> Self {
        let (staff_offset, tab_offset) = if show_classic_notation {
            (
                Self::STAFF_TOP_MARGIN,
                Self::STAFF_TOP_MARGIN + Self::STAFF_HEIGHT + Self::STAFF_BOTTOM_MARGIN,
            )
        } else {
            (0.0, 16.0)
        };
        let mut metrics = Self {
            height: 0.0,
            staff_offset,
            tab_offset,
            string_count,
        };
        metrics.height = tab_offset + metrics.tab_span() + Self::BOTTOM_PADDING;
        metrics
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
    /// drag started* - recomputing every frame from a fixed baseline plus
    /// the total raw drag distance, rather than incrementally re-snapping
    /// an already-snapped position, is what keeps a small nudge from
    /// compounding into a runaway jump.
    MovingPoints {
        /// (tick, bpm) of every selected point at drag start, in the same
        /// order as `AutomationEditorState::selected_points` at that moment.
        original_points: Vec<(Tick, f32)>,
        /// Index into `original_points` of the point actually grabbed -
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
        /// The specific note grabbed - its bar start anchors the snap grid
        /// (see `finish_canvas_drag`), and its original position is what
        /// the raw drag distance is measured from.
        anchor_note_id: u64,
        start_pos: Pos2,
        current_pos: Pos2,
    },
}

/// What finishing the current drag would do to the notes. The drop and the
/// live preview both go through this, so the preview shows exactly the result
/// the project edit will produce, including clamping against neighbours.
struct NoteDragPlan {
    edit: NoteDragEdit,
    note_ids: Vec<u64>,
    pointer: Pos2,
    snapping_disabled: bool,
}

enum NoteDragEdit {
    Move {
        offset_ticks: Tick,
        string_delta: i32,
    },
    Stretch {
        edge: StretchEdge,
        delta_ticks: Tick,
    },
}

/// Applies `plan` to `project`'s track and returns how many notes changed.
fn apply_note_drag(
    project: &mut BwxProject,
    track_id: u64,
    plan: &NoteDragPlan,
    stretch_step_ticks: Tick,
) -> usize {
    plan.note_ids
        .iter()
        .filter(|&&note_id| match plan.edit {
            NoteDragEdit::Move {
                offset_ticks,
                string_delta,
            } => project.move_note(
                track_id,
                note_id,
                offset_ticks,
                string_delta,
                &plan.note_ids,
            ),
            NoteDragEdit::Stretch { edge, delta_ticks } => {
                project.stretch_note(track_id, note_id, edge, delta_ticks, stretch_step_ticks)
            }
        })
        .count()
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
    /// The transition new points are created with, and - while any points
    /// are selected - the transition the Constant/Progressive control
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

/// A copy, cut or paste request read from the keyboard.
enum ClipboardAction {
    Copy,
    Cut,
    /// The system clipboard's text, which says whether it is still our copy.
    Paste(String),
}

/// One change from the track table's sound, volume, pan, solo and mute
/// controls.
#[derive(Clone, Copy)]
enum MixEdit {
    Program(u8),
    Volume(u8),
    Pan(i8),
    Solo(bool),
    Mute(bool),
}

/// `edit` is `None` for the frame a drag ends on, which only closes the undo
/// step the drag opened.
struct MixEvent {
    track_id: u64,
    edit: Option<MixEdit>,
    commit: bool,
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
    /// A Play request is waiting for the soundfont to finish loading.
    audio_wait: bool,
    audio_preloaded: bool,
    /// Sound pickers list every General MIDI sound, not just the ones for
    /// the track's instrument type.
    show_all_sounds: bool,
    /// Notes last copied or cut, and the text put on the system clipboard
    /// with them. egui only reports a paste while the system clipboard holds
    /// text, and the text doubles as the check that it is still our copy.
    note_clipboard: Option<(NoteClip, String)>,
    /// Counts copies so each puts different text on the system clipboard.
    copy_serial: u32,
    /// Project as it was before the volume or pan control being dragged
    /// started, so the whole drag becomes one undo step.
    mixer_edit_before: Option<BwxProject>,
    show_fretboard: bool,
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
    /// Brick or fret-number rendering; toggled by the toolbar's view button.
    view_mode: EditorViewMode,
    /// Whether the five-line classic notation staff is drawn above the tab.
    show_classic_notation: bool,
    selected_duration: DurationChoice,
    /// Augmentation dots (0 to 2) added to `selected_duration` when writing
    /// notes or changing a note's length.
    selected_dots: u8,
    active_effects: NoteEffects,
    active_velocity: u8,
    fret_to_insert: u8,
    pixels_per_tick: f32,
    /// How far the score is scrolled up, in screen points.
    scroll_y: f32,
    /// Scroll the edit cursor's row into view on the next frame.
    reveal_cursor: bool,
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
            audio_wait: false,
            audio_preloaded: false,
            show_all_sounds: false,
            note_clipboard: None,
            copy_serial: 0,
            mixer_edit_before: None,
            show_fretboard: true,
            vst_slot: Vst3HostSlot::default(),
            current_file: None,
            selected_note_id: None,
            area_selected_note_ids: Vec::new(),
            canvas_drag: CanvasDragMode::Idle,
            stretch_step_ticks: TICKS_PER_QUARTER / 4,
            drag_step_ticks: TICKS_PER_QUARTER / 4,
            view_mode: EditorViewMode::Bricks,
            show_classic_notation: true,
            selected_duration: DurationChoice::Quarter,
            selected_dots: 0,
            active_effects: NoteEffects::default(),
            active_velocity: 108,
            fret_to_insert: 3,
            pixels_per_tick: 0.07,
            scroll_y: 0.0,
            reveal_cursor: false,
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
    /// (buttons, windows, menus, text fields - everything not painted by
    /// hand on the staff canvas) follow along too. Runs every frame since
    /// it's cheap and the OS preference can change at runtime under
    /// `System`.
    fn resolve_theme(&mut self, ctx: &egui::Context) {
        let dark = match self.theme_mode {
            AppThemeMode::Dark => true,
            AppThemeMode::Bright => false,
            // `None` means egui couldn't determine the OS preference (some
            // platforms/window managers don't report one) - fall back to
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
        // Tab view keeps every note inside one bar, whatever the edit was
        // (typing, lengthening, a time signature change, ...), so anything
        // that now crosses a barline is cut into tied notes here.
        if self.view_mode == EditorViewMode::TabNumbers {
            self.project.split_bricks_at_barlines();
        }
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
        self.normalize_for_view_mode();
        self.prune_selection();
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

    /// Brings the project's notes into the shape the current view mode needs:
    /// tab view cuts bricks at barlines into tied notes, brick view joins
    /// those slices back together. Both directions are idempotent, so this
    /// is safe to call after anything that replaces or edits the project.
    /// Returns whether any note changed.
    fn normalize_for_view_mode(&mut self) -> bool {
        match self.view_mode {
            EditorViewMode::Bricks => self.project.merge_brick_ties(),
            EditorViewMode::TabNumbers => self.project.split_bricks_at_barlines(),
        }
    }

    /// Drops selected note ids that no longer exist (a merge removes the
    /// tied slices it absorbs).
    fn prune_selection(&mut self) {
        let existing: std::collections::HashSet<u64> = self
            .project
            .track(self.selected_track_id)
            .map(|track| track.notes.iter().map(|note| note.id).collect())
            .unwrap_or_default();
        self.selected_note_id = self.selected_note_id.filter(|id| existing.contains(id));
        self.area_selected_note_ids
            .retain(|id| existing.contains(id));
    }

    /// Switches between brick view and tab view. The conversion is not an
    /// undo step of its own: undo/redo re-normalize whatever they restore
    /// for the active mode, so history stays consistent across switches.
    fn set_view_mode(&mut self, mode: EditorViewMode) {
        if self.view_mode == mode {
            return;
        }
        self.view_mode = mode;
        self.canvas_drag = CanvasDragMode::Idle;
        self.normalize_for_view_mode();
        self.prune_selection();
        self.shadow_timeline.rebuild_all(&self.project);
        self.compatibility =
            CompatibilityReport::analyze(&self.project, horizon_tick(&self.project));
        self.status = match mode {
            EditorViewMode::Bricks => {
                "Brick view: notes crossing barlines are single bricks again.".to_owned()
            }
            EditorViewMode::TabNumbers => {
                "Tab view: bricks crossing barlines are now tied notes.".to_owned()
            }
        };
    }

    fn toggle_view_mode(&mut self) {
        let next = match self.view_mode {
            EditorViewMode::Bricks => EditorViewMode::TabNumbers,
            EditorViewMode::TabNumbers => EditorViewMode::Bricks,
        };
        self.set_view_mode(next);
    }

    /// Ties (or unties) the selected note(s) to the note right before them
    /// on the same string.
    fn toggle_tie_on_selection(&mut self) {
        let ids: Vec<u64> = match self.selected_note_id {
            Some(id) => vec![id],
            None => self.area_selected_note_ids.clone(),
        };
        if ids.is_empty() {
            self.status = "Select a note first, then press L to tie it.".to_owned();
            return;
        }
        let track_id = self.selected_track_id;
        let previous_project = self.project.clone();
        let changed = ids
            .iter()
            .filter(|id| self.project.toggle_user_tie(track_id, **id))
            .count();
        if changed == 0 {
            self.status = "A tie needs a note with the same fret ending exactly where this \
                one starts, on the same string."
                .to_owned();
            return;
        }
        self.record_project_edit(previous_project);
        self.refresh_after_edit(track_id);
        self.status = format!("Toggled the tie on {changed} note(s).");
    }

    fn playback_start_tick(&self) -> Tick {
        playback_start(
            self.project.track(self.selected_track_id),
            self.selected_note_id,
            &self.area_selected_note_ids,
            self.cursor.tick,
        )
    }

    /// Copies (or cuts) the selected notes. The text handed to the system
    /// clipboard is what makes the next Ctrl+V reach `paste_notes`.
    fn copy_selection(&mut self, ctx: &egui::Context, cut: bool) -> bool {
        let ids = self.selected_note_ids();
        let Some(clip) = self
            .project
            .track(self.selected_track_id)
            .and_then(|track| NoteClip::from_track(track, &ids))
        else {
            return false;
        };
        let count = clip.len();
        self.copy_serial += 1;
        let marker = format!("BetterWriter: {count} notes (copy {})", self.copy_serial);
        ctx.copy_text(marker.clone());
        self.note_clipboard = Some((clip, marker));

        if !cut {
            self.status = format!("Copied {}.", note_count(count));
            return true;
        }
        let track_id = self.selected_track_id;
        let before = self.project.clone();
        if let Some(track) = self.project.track_mut(track_id) {
            track.notes.retain(|note| !ids.contains(&note.id));
        }
        self.record_project_edit(before);
        self.selected_note_id = None;
        self.area_selected_note_ids.clear();
        self.refresh_after_edit(track_id);
        self.status = format!("Cut {}.", note_count(count));
        true
    }

    /// Pastes the copied notes at the cursor, and leaves exactly the new
    /// notes selected.
    fn paste_notes(&mut self) -> bool {
        let Some((clip, _)) = self.note_clipboard.clone() else {
            return false;
        };
        let track_id = self.selected_track_id;
        let before = self.project.clone();
        let Some(outcome) = self
            .project
            .paste_notes(track_id, &clip, self.cursor.tick)
        else {
            self.status = "Nothing to paste on this track.".to_owned();
            return false;
        };
        self.record_project_edit(before);
        self.refresh_after_edit(track_id);

        let pasted = self.with_barline_fragments(track_id, outcome.note_ids);
        self.cursor.tick = outcome.start_tick;
        self.cursor.pending_fret = None;
        self.cursor.pending_at = None;
        let count = pasted.len();
        match pasted[..] {
            [note_id] => self.select_single_note(note_id),
            _ => {
                self.selected_note_id = None;
                self.area_selected_note_ids = pasted;
            }
        }
        self.status = if outcome.bars_added > 0 {
            format!(
                "Pasted {} into {} new bar(s) at the end.",
                note_count(count),
                outcome.bars_added
            )
        } else {
            format!("Pasted {}.", note_count(count))
        };
        true
    }

    /// Tab view cuts a pasted note that crosses a barline into tied pieces;
    /// the pieces are part of what was pasted, so they join `ids`.
    fn with_barline_fragments(&self, track_id: u64, mut ids: Vec<u64>) -> Vec<u64> {
        let Some(track) = self.project.track(track_id) else {
            return ids;
        };
        loop {
            let more: Vec<u64> = track
                .notes
                .iter()
                .filter(|note| {
                    !ids.contains(&note.id)
                        && note.tied_from_previous == Some(crate::core::project::TieOrigin::BrickSplit)
                        && track
                            .tie_predecessor(note)
                            .is_some_and(|previous| ids.contains(&previous.id))
                })
                .map(|note| note.id)
                .collect();
            if more.is_empty() {
                return ids;
            }
            ids.extend(more);
        }
    }

    fn apply_mix_event(&mut self, event: MixEvent) {
        if let Some(edit) = event.edit {
            if self.mixer_edit_before.is_none() {
                self.mixer_edit_before = Some(self.project.clone());
            }
            let Some(track) = self.project.track_mut(event.track_id) else {
                return;
            };
            match edit {
                MixEdit::Program(program) => {
                    track.program = program;
                    self.status = format!("{}: {}", track.name, sound_name(program));
                }
                MixEdit::Volume(volume) => track.volume = volume,
                MixEdit::Pan(pan) => track.pan = pan,
                MixEdit::Solo(solo) => track.solo = solo,
                MixEdit::Mute(mute) => track.mute = mute,
            }
        }
        if event.commit
            && let Some(before) = self.mixer_edit_before.take()
        {
            self.record_project_edit(before);
        }
    }

    fn play(&mut self) {
        let start_tick = self.playback_start_tick();
        let events = self.shadow_timeline.snapshot();
        let has_note_to_play = events
            .iter()
            .any(|event| event.kind == MidiEventKind::NoteOn && event.tick >= start_tick);
        if !has_note_to_play {
            self.status = "No notes at or after the cursor to play.".to_owned();
            return;
        }
        let bar_number = self
            .project
            .track(self.selected_track_id)
            .map_or(1, |track| track.bar_index_at(start_tick) + 1);
        match self.audio.play(&self.project, events, start_tick) {
            Ok(()) if self.audio.is_loading() => {
                self.audio_wait = true;
                self.status =
                    "Loading the soundfont; playback starts as soon as it is ready.".to_owned();
            }
            Ok(()) => self.status = format!("Playing from bar {bar_number}."),
            Err(err) => self.status = format!("Audio unavailable: {err}"),
        }
    }

    fn stop(&mut self) {
        self.audio_wait = false;
        match self.audio.stop() {
            Ok(()) => self.status = "Playback stopped.".to_owned(),
            Err(err) => self.status = format!("Stop command failed: {err}"),
        }
    }

    /// Space bar: stop when something is sounding (or about to), otherwise
    /// play from the selection or edit cursor.
    fn toggle_playback(&mut self) {
        if self.audio.is_playing() {
            self.stop();
        } else {
            self.play();
        }
    }

    /// Per-frame audio housekeeping: hands a finished soundfont load to the
    /// audio thread and reports how a deferred Play request turned out.
    fn poll_audio(&mut self, ctx: &egui::Context) {
        if !self.audio_preloaded {
            self.audio_preloaded = true;
            self.audio.preload();
        }
        let busy = self.audio.poll();
        self.audio.sync_mix(&self.project);
        if self.audio_wait && !self.audio.is_loading() {
            self.audio_wait = false;
            self.status = if self.audio.is_playing() {
                "Playback started.".to_owned()
            } else {
                self.audio.status().to_owned()
            };
        }
        if busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
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
        self.area_selected_note_ids.clear();
        self.normalize_for_view_mode();
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

    /// Loads the built-in demo project (the Start Menu's "Demo Project"
    /// button).
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
            .track(self.selected_track_id)
            .and_then(|track| {
                let note = track.notes.iter().find(|note| note.id == note_id)?;
                let fret = (note.fret as i16 + delta).clamp(0, track.fret_count as i16) as u8;
                (fret != note.fret).then_some(fret)
            });
        if let Some(fret) = changed {
            self.project
                .set_note_fret(self.selected_track_id, note_id, fret);
            self.record_project_edit(previous_project);
            self.refresh_after_edit(self.selected_track_id);
            self.status = format!("Changed selected note to fret {fret}.");
            true
        } else {
            false
        }
    }

    /// Moves the selected note one value shorter or longer, keeping its dots:
    /// a dotted quarter stepped down becomes a dotted eighth. A length that
    /// is not a standard one starts from the nearest plain value.
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
        let (current, dots) = DurationChoice::detect(current_ticks).unwrap_or_else(|| {
            let nearest = DurationChoice::ALL
                .into_iter()
                .min_by_key(|duration| (duration.ticks() - current_ticks).abs())
                .unwrap_or(DurationChoice::Quarter);
            (nearest, 0)
        });
        let current_index = DurationChoice::ALL
            .iter()
            .position(|duration| *duration == current)
            .unwrap_or(2) as i32;
        let new_index =
            (current_index + steps).clamp(0, DurationChoice::ALL.len() as i32 - 1) as usize;
        let duration = DurationChoice::ALL[new_index];
        let previous_project = self.project.clone();
        if self.project.set_note_duration_fluid(
            self.selected_track_id,
            note_id,
            duration.ticks_dotted(dots),
        ) {
            self.selected_duration = duration;
            self.selected_dots = dots;
            self.record_project_edit(previous_project);
            self.refresh_after_edit(self.selected_track_id);
            self.status = format!(
                "Changed selected note length to {}.",
                duration.label_dotted(dots)
            );
            true
        } else {
            false
        }
    }

    /// If a fret digit has been typed and ~0.3s has passed with no further
    /// digit, auto-commits it as a note - matching Guitar Pro, so a
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

    /// Onset ticks of every beat of the selected track, in order: the
    /// positions the tab-view cursor can stop on.
    fn all_beat_ticks(&self) -> Vec<Tick> {
        let Some(track) = self.project.track(self.selected_track_id) else {
            return Vec::new();
        };
        let mut ticks = Vec::new();
        let mut cursor: Tick = 0;
        for _ in 0..track.bar_count.max(1) {
            let bounds = track.measure_bounds_at(cursor);
            cursor = bounds.1;
            ticks.extend(self.bar_beat_ticks(track, bounds));
        }
        ticks
    }

    /// Moves the tab-view cursor to the next (`forward`) or previous beat.
    fn move_cursor_beat(&mut self, forward: bool) {
        let beats = self.all_beat_ticks();
        let current = self.cursor.tick;
        let target = if forward {
            beats.iter().copied().find(|tick| *tick > current)
        } else {
            beats.iter().copied().rev().find(|tick| *tick < current)
        };
        if let Some(tick) = target {
            self.cursor.tick = tick;
        }
        self.cursor.pending_fret = None;
        self.cursor.pending_at = None;
    }

    /// After writing a note in tab view, parks the cursor on the first beat
    /// at or after the note's end (the rest slot that follows it, or the
    /// next bar's first beat).
    fn advance_cursor_past(&mut self, note_end: Tick) {
        let target = self
            .all_beat_ticks()
            .into_iter()
            .find(|tick| *tick >= note_end)
            .unwrap_or(note_end);
        self.cursor.tick = target;
    }

    /// Advance the cursor by the currently selected note duration. If the
    /// cursor would cross into a new measure whose time signature differs,
    /// `measure_bounds_at` keeps it inside the same bar so writing stays sane.
    fn advance_cursor(&mut self) {
        let step = self.entry_ticks();
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
        self.write_fret_at_cursor(true);
    }

    /// Writes the pending (or default) fret at the cursor. The cursor then
    /// moves past the note when `advance` is set; otherwise it stays so more
    /// notes can be added to the same beat.
    fn write_fret_at_cursor(&mut self, advance: bool) {
        let track_id = self.selected_track_id;
        let Some(track) = self.project.track(track_id) else {
            return;
        };
        let tick = self.cursor.tick.max(0);
        if tick >= track.bars_end_tick() {
            self.cursor.pending_fret = None;
            self.status =
                "No bar there yet - add a bar first (Ctrl+B or right-click > Bar > Add Bar)."
                    .to_owned();
            return;
        }
        let string_index = self
            .cursor
            .string_index
            .min(track.tuning.len().saturating_sub(1));
        let fret = self.cursor.pending_fret.unwrap_or(self.fret_to_insert);
        let mut duration = self.entry_ticks();
        let mut shortened_to: Option<String> = None;
        if self.view_mode == EditorViewMode::TabNumbers {
            // Tab view enters notes the TuxGuitar way: a note joining a beat
            // that already has notes takes that beat's length, and a note
            // in an empty slot takes the duration being entered, cut down
            // to the longest standard length that fits before the next beat
            // with notes or the barline.
            let (_, bar_end) = track.measure_bounds_at(tick);
            let chord_duration = track
                .notes
                .iter()
                .filter(|note| note.abs_tick == tick)
                .map(|note| note.duration_ticks)
                .min();
            match chord_duration {
                Some(shared) => duration = shared,
                None => {
                    let next_onset = track
                        .notes
                        .iter()
                        .filter(|note| note.abs_tick > tick && note.abs_tick < bar_end)
                        .map(|note| note.abs_tick)
                        .min()
                        .unwrap_or(bar_end);
                    let room = next_onset - tick;
                    if duration > room {
                        let Some((choice, dots)) = DurationChoice::longest_fitting(room) else {
                            self.cursor.pending_fret = None;
                            self.status = "No room for a note here.".to_owned();
                            return;
                        };
                        duration = choice.ticks_dotted(dots);
                        shortened_to = Some(choice.label_dotted(dots));
                    }
                }
            }
        }
        let mut advance_to = tick + duration;
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
            self.project.set_note_fret(track_id, id, fret);
            if let Some(track) = self.project.track_mut(track_id)
                && let Some(note) = track.notes.iter_mut().find(|n| n.id == id)
            {
                note.velocity = self.active_velocity;
                note.effects = self.active_effects.clone();
                advance_to = note.end_tick();
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
            self.status = match shortened_to {
                Some(label) => format!(
                    "Wrote fret {fret} as a {label} note: that is all that fits before the next \
                     note or the barline. Tie notes with L to sustain across."
                ),
                None => format!(
                    "Wrote fret {fret} at tick {tick} on string {}.",
                    string_index + 1
                ),
            };
        }

        self.cursor.pending_fret = None;
        self.cursor.pending_at = None;
        self.record_project_edit(previous_project);
        self.refresh_after_edit(track_id);
        if !advance {
            return;
        }
        if self.view_mode == EditorViewMode::TabNumbers {
            self.advance_cursor_past(advance_to);
        } else {
            self.advance_cursor();
        }
    }

    /// Enters the note under a fretboard click on the cursor's beat. A note
    /// that already sits exactly there is removed instead, so a click toggles.
    /// `stay_on_beat` keeps the cursor in place for building chords.
    fn click_fretboard(&mut self, string_index: usize, fret: u8, stay_on_beat: bool) {
        self.reveal_cursor = true;
        self.cursor.string_index = string_index;
        self.cursor.pending_fret = None;
        self.cursor.pending_at = None;
        let tick = self.cursor.tick;
        let already_there = self.project.track(self.selected_track_id).is_some_and(|track| {
            track.notes.iter().any(|note| {
                note.abs_tick == tick && note.string_index == string_index && note.fret == fret
            })
        });
        if already_there {
            self.delete_at_cursor();
            return;
        }
        self.cursor.pending_fret = Some(fret);
        self.write_fret_at_cursor(!stay_on_beat);
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

    /// Length in ticks of the note being entered: the selected value with its
    /// dots.
    fn entry_ticks(&self) -> Tick {
        self.selected_duration.ticks_dotted(self.selected_dots)
    }

    fn entry_label(&self) -> String {
        self.selected_duration.label_dotted(self.selected_dots)
    }

    /// Switches one dot (or two) on or off for the notes being written and,
    /// when a single note is selected, gives that note the new length.
    fn toggle_dots(&mut self, dots: u8) {
        self.selected_dots = if self.selected_dots == dots { 0 } else { dots };
        self.status = format!("Note duration: {}", self.entry_label());
        self.apply_selected_duration();
    }

    fn cycle_duration(&mut self, steps: i32) {
        let idx = DurationChoice::ALL
            .iter()
            .position(|d| *d == self.selected_duration)
            .unwrap_or(2) as i32;
        let len = DurationChoice::ALL.len() as i32;
        let next = ((idx + steps).rem_euclid(len)) as usize;
        self.selected_duration = DurationChoice::ALL[next];
        self.status = format!("Note duration: {}", self.entry_label());
    }

    /// Central keyboard controller. Drives the entire write workflow so the
    /// mouse is optional. Returns `true` if any key was consumed so the caller
    /// can request a repaint when the score changes.
    ///
    /// Bindings (mirrors TuxGuitar's tab-entry feel):
    ///   - Digits 0-9: build the fret number (multi-digit allowed)
    ///   - Enter: place the buffered (or default) fret at the cursor
    ///   - Space: play from the selection or cursor, or stop when already playing
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
        let typing_in_text_field = ctx.egui_wants_keyboard_input();

        let mut clipboard_actions: Vec<ClipboardAction> = Vec::new();
        ctx.input(|i| {
            // egui reports Ctrl+C, Ctrl+X and Ctrl+V as events of their own
            // instead of key presses.
            if !typing_in_text_field {
                for event in &i.events {
                    match event {
                        egui::Event::Copy => clipboard_actions.push(ClipboardAction::Copy),
                        egui::Event::Cut => clipboard_actions.push(ClipboardAction::Cut),
                        egui::Event::Paste(text) => {
                            clipboard_actions.push(ClipboardAction::Paste(text.clone()));
                        }
                        _ => {}
                    }
                }
            }
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

            // Fret digits - always available so the user can start typing
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
                if self.view_mode == EditorViewMode::TabNumbers {
                    self.move_cursor_beat(true);
                } else {
                    self.advance_cursor();
                }
                self.selected_note_id = None;
                changed = true;
            }
            if adjust_selected_note && i.key_pressed(egui::Key::ArrowLeft) {
                changed |= self.step_selected_note_duration(1);
            } else if i.key_pressed(egui::Key::ArrowLeft) {
                if self.view_mode == EditorViewMode::TabNumbers {
                    self.move_cursor_beat(false);
                } else {
                    let step = self.entry_ticks();
                    self.cursor.tick = (self.cursor.tick - step).max(0);
                    self.cursor.pending_fret = None;
                    self.cursor.pending_at = None;
                }
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
            if i.key_pressed(egui::Key::Enter) {
                self.commit_fret_at_cursor();
                changed = true;
            }
            // Space is the transport shortcut. It is ignored while a text
            // field owns the keyboard, so typing a space never starts playback.
            if !typing_in_text_field && i.key_pressed(egui::Key::Space) {
                self.toggle_playback();
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

            // Duration cycling: `-` -> longer, `+`/`=` -> shorter
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
                    self.status = format!("Note duration: {}", self.entry_label());
                    changed = true;
                }
            }

            // `.` dots the note being written (and the selected note);
            // `Ctrl+.` double-dots it.
            // The modifiers come from the key event itself, so a fast Ctrl+.
            // is not read as a plain `.` when Ctrl is already up again by the
            // end of the frame.
            for event in &i.events {
                if let egui::Event::Key {
                    key: egui::Key::Period,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } = event
                    && !typing_in_text_field
                    && !modifiers.alt
                {
                    self.toggle_dots(if modifiers.command { 2 } else { 1 });
                    changed = true;
                }
            }

            if i.key_pressed(egui::Key::L) && !i.modifiers.command && !i.modifiers.shift {
                self.toggle_tie_on_selection();
                changed = true;
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

        for action in clipboard_actions {
            changed |= match action {
                ClipboardAction::Copy => self.copy_selection(ctx, false),
                ClipboardAction::Cut => self.copy_selection(ctx, true),
                ClipboardAction::Paste(text) => {
                    let ours = self
                        .note_clipboard
                        .as_ref()
                        .is_some_and(|(_, marker)| *marker == text);
                    ours && self.paste_notes()
                }
            };
        }

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
                if self.new_project_dialog.is_none() && self.handle_keyboard(root.ctx()) {
                    self.reveal_cursor = true;
                }
                self.poll_audio(root.ctx());

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
                    .exact_size(PALETTE_WIDTH)
                    .frame(egui::Frame::NONE.fill(self.theme.palette_background))
                    .show_inside(root, |ui| self.show_tux_palette(ui));

                if self.show_fretboard {
                    let strings = self
                        .project
                        .track(self.selected_track_id)
                        .map_or(6, |track| track.tuning.len());
                    egui::Panel::bottom("fretboard")
                        .exact_size(fretboard::panel_height(strings))
                        .frame(egui::Frame::NONE.fill(self.theme.fretboard_background))
                        .show_inside(root, |ui| self.show_fretboard(ui));
                }

                egui::Panel::bottom("track_table")
                    .resizable(true)
                    .default_size(110.0)
                    .size_range(72.0..=420.0)
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
                ui.separator();
                let has_selection = !self.selected_note_ids().is_empty();
                let can_paste = self.note_clipboard.is_some();
                if ui
                    .add_enabled(has_selection, egui::Button::new("Cut\tCtrl+X"))
                    .clicked()
                {
                    let ctx = ui.ctx().clone();
                    self.copy_selection(&ctx, true);
                    ui.close();
                }
                if ui
                    .add_enabled(has_selection, egui::Button::new("Copy\tCtrl+C"))
                    .clicked()
                {
                    let ctx = ui.ctx().clone();
                    self.copy_selection(&ctx, false);
                    ui.close();
                }
                if ui
                    .add_enabled(can_paste, egui::Button::new("Paste\tCtrl+V"))
                    .clicked()
                {
                    self.paste_notes();
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
                ui.separator();
                ui.label(egui::RichText::new("Notes").strong());
                if ui
                    .radio(self.view_mode == EditorViewMode::Bricks, "Brick view")
                    .clicked()
                {
                    self.set_view_mode(EditorViewMode::Bricks);
                    ui.close();
                }
                if ui
                    .radio(
                        self.view_mode == EditorViewMode::TabNumbers,
                        "Tab view (fret numbers)",
                    )
                    .clicked()
                {
                    self.set_view_mode(EditorViewMode::TabNumbers);
                    ui.close();
                }
                ui.separator();
                if ui
                    .checkbox(&mut self.show_classic_notation, "Classic notation")
                    .clicked()
                {
                    ui.close();
                }
                if ui
                    .checkbox(&mut self.show_fretboard, "Fretboard")
                    .clicked()
                {
                    ui.close();
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
                ui.label(egui::RichText::new("Keyboard - Tab Writing").strong());
                ui.separator();
                ui.label("0-9 ............ type fret (multi-digit, e.g. 1 then 2 = 12)");
                ui.label("Enter ... place the buffered fret at the cursor");
                ui.label("Space ... play from the cursor or selection / stop");
                ui.label("Fretboard click ... write the note; Shift+click adds to the same beat");
                ui.label("Mouse wheel ... zoom around the pointer; Shift+wheel scrolls");
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
                ui.label("L ............... tie / untie the selected note to the one before it");
                ui.separator();
                ui.label(egui::RichText::new("Keyboard - Bars").strong());
                ui.label("Ctrl+Ins / Ctrl+Shift+B ... insert a bar before the selected bar");
                ui.label("Ctrl+B .......... add a bar after the selected bar");
                ui.label("Ctrl+Del ........ delete the selected bar");
                ui.separator();
                ui.label(egui::RichText::new("Mouse - Staff canvas").strong());
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
            // Delete/Backspace removes the selected point(s) - but only when
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
        // egui shrink-wraps a Window's height to whatever its content uses
        // unless the content claims the space, so a vertical resize is
        // discarded on the next frame and the handle looks like it only
        // drags sideways.
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
                    // `top_down` is load-bearing, not decorative: a plain
                    // `ui.allocate_ui` inherits this row's left-to-right
                    // layout, which disables text wrapping, so the
                    // point-settings text spills out of its column and
                    // squeezes the graph column.
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
        // A stale selection (e.g. the point got deleted from under it) -
        // drop it and fall through to the "0 selected" presentation.
        if selected_count == 1 && single_point.is_none() {
            state.selected_points.clear();
        }
        let selected_count = state.selected_points.len();

        ui.label("Position:");
        ui.label(
            egui::RichText::new(match selected_count {
                0 => "-".to_owned(),
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
                    "The first point can't be removed - it sets the project's starting tempo"
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

    /// The Constant/Progressive transition control. Always visible - not
    /// nested inside a "1 point selected" branch - because it does double
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
        // reliably fills whatever room the window currently has - resizing
        // the window taller gives a taller graph - instead of shrinking to
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
                // little mapping closures never hold a borrow of `state` -
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
                            // "close enough" to a grid line right now -
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
            if small_tool_button(ui, ">", "Play from the cursor or selection (Space)").clicked() {
                self.play();
            }
            if small_tool_button(ui, "[]", "Stop (Space)").clicked() {
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
            let (view_label, view_hint) = match self.view_mode {
                EditorViewMode::Bricks => (
                    "Tab",
                    "Switch to fret-number tab view. Bricks crossing barlines become tied notes.",
                ),
                EditorViewMode::TabNumbers => (
                    "Bricks",
                    "Switch back to brick view. Ties made by splitting bricks are joined again.",
                ),
            };
            if small_tool_button(ui, view_label, view_hint).clicked() {
                self.toggle_view_mode();
            }
            if ui
                .add_sized(
                    [62.0, 22.0],
                    egui::Button::new("Notation").selected(self.show_classic_notation),
                )
                .on_hover_text("Show or hide the classic notation staff above the tablature")
                .clicked()
            {
                self.show_classic_notation = !self.show_classic_notation;
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
            for (dots, label, tip) in [
                (1_u8, ".", "Dotted: adds half the length again (.)"),
                (2_u8, "..", "Double-dotted: adds three quarters of the length (Ctrl+.)"),
            ] {
                if ui
                    .add_sized(
                        [if dots == 1 { 20.0 } else { 26.0 }, 22.0],
                        egui::Button::new(label).selected(self.selected_dots == dots),
                    )
                    .on_hover_text(tip)
                    .clicked()
                {
                    self.selected_dots = if self.selected_dots == dots { 0 } else { dots };
                }
            }
            ui.add(
                egui::DragValue::new(&mut self.fret_to_insert)
                    .range(0..=36)
                    .prefix("default fret "),
            )
            .on_hover_text("Fret used when you press Enter without typing a number first");
            ui.add(egui::Slider::new(
                    &mut self.pixels_per_tick,
                    MIN_PIXELS_PER_TICK..=MAX_PIXELS_PER_TICK,
                )
                .text("scale"));
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
        let theme = self.theme;
        ui.spacing_mut().item_spacing = Vec2::splat(PALETTE_GAP);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(8.0);

                palette_row(ui, |ui| {
                    let signature = palette_button(
                        ui,
                        &theme,
                        PaletteIcon::TimeSignature,
                        false,
                        true,
                        "Time signature",
                    );
                    if signature.clicked() {
                        self.sync_time_signature_picker();
                    }
                    egui::Popup::menu(&signature).show(|ui| self.show_time_signature_picker(ui));
                    for (icon, name) in [
                        (PaletteIcon::KeySignature, "Key signature"),
                        (PaletteIcon::Clef, "Clef"),
                        (PaletteIcon::TripletFeel, "Triplet feel"),
                        (PaletteIcon::RepeatOpen, "Open repeat"),
                        (PaletteIcon::RepeatClose, "Close repeat"),
                    ] {
                        planned_palette_button(ui, &theme, icon, name);
                    }
                });
                palette_row(ui, |ui| {
                    if palette_button(
                        ui,
                        &theme,
                        PaletteIcon::AddBarAfter,
                        false,
                        true,
                        "Add a bar after the selected bar (Ctrl+B)",
                    )
                    .clicked()
                    {
                        self.add_bar_after_selected();
                    }
                    if palette_button(
                        ui,
                        &theme,
                        PaletteIcon::InsertBarBefore,
                        false,
                        true,
                        "Insert a bar before the selected bar (Ctrl+Shift+B)",
                    )
                    .clicked()
                    {
                        self.insert_bar_before_selected();
                    }
                    if palette_button(
                        ui,
                        &theme,
                        PaletteIcon::DeleteBar,
                        false,
                        true,
                        "Delete the selected bar (Ctrl+Del)",
                    )
                    .clicked()
                    {
                        self.delete_selected_bar();
                    }
                });
                palette_separator(ui, &theme);

                palette_row(ui, |ui| {
                    for (slot, duration) in DurationChoice::ALL.into_iter().enumerate() {
                        let tooltip =
                            format!("{} note (Ctrl+{})", duration_name(duration), slot + 1);
                        if palette_button(
                            ui,
                            &theme,
                            duration_icon(duration),
                            self.selected_duration == duration,
                            true,
                            &tooltip,
                        )
                        .clicked()
                        {
                            self.selected_duration = duration;
                            self.apply_selected_duration();
                        }
                    }
                    if palette_button(
                        ui,
                        &theme,
                        PaletteIcon::Dotted,
                        self.selected_dots == 1,
                        true,
                        "Dotted note: adds half the length again (.)",
                    )
                    .clicked()
                    {
                        self.toggle_dots(1);
                    }
                    if palette_button(
                        ui,
                        &theme,
                        PaletteIcon::DoubleDotted,
                        self.selected_dots == 2,
                        true,
                        "Double-dotted note: adds three quarters of the length (Ctrl+.)",
                    )
                    .clicked()
                    {
                        self.toggle_dots(2);
                    }
                });
                palette_row(ui, |ui| {
                    planned_palette_button(ui, &theme, PaletteIcon::Tuplet, "Tuplet");
                    if palette_button(
                        ui,
                        &theme,
                        PaletteIcon::Tie,
                        false,
                        true,
                        "Tie the selected note to the one before it (L)",
                    )
                    .clicked()
                    {
                        self.toggle_tie_on_selection();
                    }
                    planned_palette_button(ui, &theme, PaletteIcon::Fermata, "Fermata");
                });
                palette_separator(ui, &theme);

                palette_row(ui, |ui| {
                    for (label, velocity) in DYNAMICS {
                        let tooltip = format!("{label} (velocity {velocity})");
                        if palette_button(
                            ui,
                            &theme,
                            PaletteIcon::Dynamic(label),
                            self.active_velocity == velocity,
                            true,
                            &tooltip,
                        )
                        .clicked()
                        {
                            self.active_velocity = velocity;
                            self.apply_velocity_to_selected();
                        }
                    }
                });
                palette_row(ui, |ui| {
                    planned_palette_button(ui, &theme, PaletteIcon::Crescendo, "Crescendo");
                    planned_palette_button(ui, &theme, PaletteIcon::Decrescendo, "Decrescendo");
                });
                palette_separator(ui, &theme);

                for row in EffectKind::ALL.chunks(PALETTE_COLUMNS) {
                    palette_row(ui, |ui| {
                        for &kind in row {
                            let active = kind.is_set(&self.active_effects);
                            if palette_button(ui, &theme, kind.icon(), active, true, kind.tooltip())
                                .clicked()
                            {
                                self.toggle_active_effect(kind);
                            }
                        }
                    });
                }
                palette_row(ui, |ui| {
                    for (icon, name) in [
                        (PaletteIcon::Bend, "Bend"),
                        (PaletteIcon::Harmonic, "Harmonic"),
                        (PaletteIcon::Trill, "Trill"),
                        (PaletteIcon::Grace, "Grace note"),
                        (PaletteIcon::TremoloBar, "Tremolo bar"),
                    ] {
                        planned_palette_button(ui, &theme, icon, name);
                    }
                });
                palette_separator(ui, &theme);

                palette_row(ui, |ui| {
                    planned_palette_button(ui, &theme, PaletteIcon::Text, "Beat text");
                    planned_palette_button(ui, &theme, PaletteIcon::Chord, "Chord diagram");
                });
                ui.add_space(8.0);
            });
        let edge = ui.max_rect();
        ui.painter().vline(
            edge.right() - 0.5,
            edge.y_range(),
            Stroke::new(1.0_f32, theme.palette_separator),
        );
    }

    /// Numerator and denominator of the time signature applied to the bar
    /// under the edit cursor. Shared by the context menu and the palette.
    fn show_time_signature_picker(&mut self, ui: &mut egui::Ui) {
        ui.label("Applies to the whole bar containing the cursor");
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.time_signature_numerator)
                    .range(1..=32)
                    .prefix(" "),
            );
            ui.label("/");
            // A list-scrolling `DragValue` rather than a `ComboBox`: a
            // ComboBox opens its own popup, which conflicts with being nested
            // inside a menu. It steps through {1,2,4,8,16,32} instead of every
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
    }

    fn show_track_table(&mut self, ui: &mut egui::Ui) {
        let show_all_sounds = self.show_all_sounds;
        let mut mix_events: Vec<MixEvent> = Vec::new();
        let full_size = ui.available_size();
        // The content gets the panel's whole height (a plain `horizontal`
        // is one row tall), so the separator and the scrolling track list
        // reach the bottom of the panel.
        ui.allocate_ui_with_layout(
            full_size,
            egui::Layout::left_to_right(egui::Align::TOP),
            |ui| {
                ui.vertical(|ui| {
                    if let Some(track) = self.project.track(self.selected_track_id) {
                        ui.label(format!("{} {}", track.name, track.program));
                        ui.add(
                            egui::Slider::new(&mut self.active_velocity, 1..=127).text("velocity"),
                        );
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.x = TRACK_COLUMN_GAP;
                    let sound_column_start: f32 = TRACK_COLUMNS[..SOUND_COLUMN]
                        .iter()
                        .map(|(_, width)| width)
                        .sum::<f32>()
                        + TRACK_COLUMN_GAP * (SOUND_COLUMN as f32 - 1.0);
                    ui.horizontal(|ui| {
                        ui.add_space(sound_column_start);
                        ui.checkbox(&mut self.show_all_sounds, "All sounds")
                            .on_hover_text("List every sound, not only those for the instrument type");
                    });
                    ui.horizontal(|ui| {
                        for (heading, width) in TRACK_COLUMNS {
                            table_cell(ui, width, |ui| ui.strong(heading));
                        }
                    });
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                        .show(ui, |ui| {
                            let mut selected_after = None;
                            for track in &self.project.tracks {
                                ui.horizontal(|ui| {
                                    let selected = track.id == self.selected_track_id;
                                    table_cell(ui, TRACK_COLUMNS[0].1, |ui| {
                                        if ui
                                            .selectable_label(selected, track.id.to_string())
                                            .clicked()
                                        {
                                            selected_after = Some(track.id);
                                        }
                                    });
                                    table_cell(ui, TRACK_COLUMNS[1].1, |ui| {
                                        if ui
                                            .selectable_label(track.solo, "S")
                                            .on_hover_text("Solo")
                                            .clicked()
                                        {
                                            mix_events.push(MixEvent {
                                                track_id: track.id,
                                                edit: Some(MixEdit::Solo(!track.solo)),
                                                commit: true,
                                            });
                                        }
                                    });
                                    table_cell(ui, TRACK_COLUMNS[2].1, |ui| {
                                        if ui
                                            .selectable_label(track.mute, "M")
                                            .on_hover_text("Mute")
                                            .clicked()
                                        {
                                            mix_events.push(MixEvent {
                                                track_id: track.id,
                                                edit: Some(MixEdit::Mute(!track.mute)),
                                                commit: true,
                                            });
                                        }
                                    });
                                    table_cell(ui, TRACK_COLUMNS[3].1, |ui| {
                                        if ui.selectable_label(selected, &track.name).clicked() {
                                            selected_after = Some(track.id);
                                        }
                                    });
                                    table_cell(ui, TRACK_COLUMNS[4].1, |ui| {
                                        egui::ComboBox::from_id_salt(("track_sound", track.id))
                                            .selected_text(sound_name(track.program))
                                            .width(TRACK_COLUMNS[4].1 - 24.0)
                                            .height(360.0)
                                            .show_ui(ui, |ui| {
                                                for program in sound_choices(
                                                    track.stringed_variant,
                                                    track.program,
                                                    show_all_sounds,
                                                ) {
                                                    let label =
                                                        format!("{program}  {}", sound_name(program));
                                                    if ui
                                                        .selectable_label(
                                                            program == track.program,
                                                            label,
                                                        )
                                                        .clicked()
                                                    {
                                                        mix_events.push(MixEvent {
                                                            track_id: track.id,
                                                            edit: Some(MixEdit::Program(program)),
                                                            commit: true,
                                                        });
                                                    }
                                                }
                                            });
                                    });
                                    table_cell(ui, TRACK_COLUMNS[5].1, |ui| {
                                        let mut volume = track.volume;
                                        let response = ui.add(
                                            egui::DragValue::new(&mut volume)
                                                .range(0..=127)
                                                .speed(0.5),
                                        );
                                        push_mix_event(
                                            &mut mix_events,
                                            track.id,
                                            &response,
                                            MixEdit::Volume(volume),
                                        );
                                    });
                                    table_cell(ui, TRACK_COLUMNS[6].1, |ui| {
                                        let mut pan = track.pan;
                                        let response = ui.add(
                                            egui::DragValue::new(&mut pan)
                                                .range(-64..=63)
                                                .speed(0.5)
                                                .custom_formatter(|value, _| {
                                                    pan_label(value as i8)
                                                })
                                                .custom_parser(parse_pan),
                                        );
                                        push_mix_event(
                                            &mut mix_events,
                                            track.id,
                                            &response,
                                            MixEdit::Pan(pan),
                                        );
                                    });
                                    table_cell(ui, TRACK_COLUMNS[7].1, |ui| {
                                        ui.label((track.channel + 1).to_string())
                                    });
                                });
                            }
                            if let Some(track_id) = selected_after {
                                self.selected_track_id = track_id;
                                self.selected_note_id = None;
                                self.reset_cursor_for_track(track_id);
                            }
                        });
                });
            },
        );
        for event in mix_events {
            self.apply_mix_event(event);
        }
    }

    /// Clickable fretboard for the selected track. A click writes the note
    /// at the edit cursor on that string and fret, and the notes sounding at
    /// the cursor are marked on the neck.
    fn show_fretboard(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, self.theme.fretboard_background);
        let Some(track) = self.project.track(self.selected_track_id) else {
            return;
        };
        if track.tuning.is_empty() {
            return;
        }
        let theme = self.theme;
        let tuning = track.tuning.clone();
        let fret_count = track.fret_count.max(1);
        let cursor_tick = self.cursor.tick;
        let cursor_string = self.cursor.string_index.min(tuning.len() - 1);
        let markers: Vec<(usize, u8, bool)> = track
            .notes
            .iter()
            .filter(|note| note.abs_tick <= cursor_tick && cursor_tick < note.end_tick())
            .map(|note| (note.string_index, note.fret, note.abs_tick == cursor_tick))
            .collect();

        let frame = FretboardFrame::new(rect, tuning.len(), fret_count);
        let hovered = response.hover_pos().and_then(|pos| frame.hit(pos));
        let strings = tuning.len();

        painter.rect_filled(frame.board, 2.0, theme.fretboard_wood);
        let inlay_radius = (frame.string_gap() * 0.3).clamp(2.5, 4.5);
        for fret in 1..=fret_count {
            let x = frame.cell_center_x(fret);
            let middle = frame.board.center().y;
            let offset = frame.board.height() * 0.2;
            match fretboard::inlay_at(fret) {
                Inlay::None => {}
                Inlay::Single => {
                    painter.circle_filled(Pos2::new(x, middle), inlay_radius, theme.fretboard_inlay);
                }
                Inlay::Double => {
                    for y in [middle - offset, middle + offset] {
                        painter.circle_filled(Pos2::new(x, y), inlay_radius, theme.fretboard_inlay);
                    }
                }
            }
        }

        let band = frame.cell_rect(cursor_string, 0);
        painter.rect_filled(
            Rect::from_min_max(band.min, Pos2::new(frame.board.right(), band.max.y)),
            0.0,
            theme.accent_color.gamma_multiply(0.14),
        );
        if let Some((string, fret)) = hovered {
            painter.rect_filled(
                frame.cell_rect(string, fret),
                2.0,
                theme.accent_color.gamma_multiply(0.3),
            );
        }

        for fret in 1..=fret_count {
            let x = frame.wire_x(fret);
            painter.line_segment(
                [Pos2::new(x, frame.board.top()), Pos2::new(x, frame.board.bottom())],
                Stroke::new(1.4_f32, theme.fretboard_wire),
            );
        }
        let nut = Rect::from_min_max(
            Pos2::new(frame.board.left() - 4.0, frame.board.top()),
            Pos2::new(frame.board.left(), frame.board.bottom()),
        );
        painter.rect_filled(nut, 1.0, theme.fretboard_nut);

        for (string, &pitch) in tuning.iter().enumerate() {
            let y = frame.string_y(string);
            let thickness = 0.9 + 1.9 * string as f32 / (strings.max(2) - 1) as f32;
            painter.line_segment(
                [
                    Pos2::new(frame.open_zone.left(), y),
                    Pos2::new(frame.board.right(), y),
                ],
                Stroke::new(thickness, theme.fretboard_string),
            );
            painter.circle_stroke(
                frame.cell_center(string, 0),
                frame.marker_radius() * 0.55,
                Stroke::new(1.0_f32, theme.fretboard_string.gamma_multiply(0.45)),
            );
            painter.text(
                Pos2::new(frame.label_right, y),
                Align2::RIGHT_CENTER,
                string_label(pitch),
                FontId::monospace(10.0),
                theme.fretboard_inlay,
            );
        }

        let number_y = frame.board.bottom() + 9.0;
        for fret in 1..=fret_count {
            let (left, right) = frame.cell_x_range(fret);
            if right - left >= 15.0 || fret == 1 || fretboard::inlay_at(fret) != Inlay::None {
                painter.text(
                    Pos2::new((left + right) * 0.5, number_y),
                    Align2::CENTER_CENTER,
                    fret.to_string(),
                    FontId::monospace(9.0),
                    theme.fretboard_inlay,
                );
            }
        }

        let radius = frame.marker_radius();
        let label_font = FontId::monospace((radius * 1.35).clamp(8.0, 11.0));
        for (string, fret, onset) in markers {
            if string >= strings {
                continue;
            }
            let center = frame.cell_center(string, fret);
            if onset {
                painter.circle_filled(center, radius, theme.accent_color);
            } else {
                painter.circle_stroke(center, radius, Stroke::new(1.5_f32, theme.accent_color));
            }
            painter.text(
                center,
                Align2::CENTER_CENTER,
                fret.to_string(),
                label_font.clone(),
                if onset { theme.sheet_background } else { theme.accent_color },
            );
        }

        if let Some((string, fret)) = hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            let pitch = tuning[string] as i32 + fret as i32;
            response.clone().on_hover_text_at_pointer(format!(
                "{}{}  (string {}, fret {})",
                string_label(pitch.clamp(0, 127) as u8),
                pitch / 12 - 1,
                string + 1,
                fret
            ));
        }
        if response.clicked()
            && let Some((string, fret)) = response.interact_pointer_pos().and_then(|pos| frame.hit(pos))
        {
            // Modifiers carried by the release event survive a quick tap where
            // Shift is already up again by the end of the frame.
            let stay_on_beat = ui.input(|input| {
                input.modifiers.shift
                    || input.events.iter().any(|event| {
                        matches!(event, egui::Event::PointerButton { modifiers, .. } if modifiers.shift)
                    })
            });
            self.click_fretboard(string, fret, stay_on_beat);
        }
    }

    fn apply_selected_duration(&mut self) {
        if let Some((track_id, note_id)) = self.selected_note_mut() {
            let previous_project = self.project.clone();
            if self.project.set_note_duration_fluid(
                track_id,
                note_id,
                self.entry_ticks(),
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
        let duration = self.entry_ticks();
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

    /// The single selected note plus every area-selected note.
    fn selected_note_ids(&self) -> Vec<u64> {
        self.selected_note_id
            .into_iter()
            .chain(self.area_selected_note_ids.iter().copied())
            .collect()
    }

    fn apply_velocity_to_selected(&mut self) {
        let ids = self.selected_note_ids();
        if ids.is_empty() {
            return;
        }
        let track_id = self.selected_track_id;
        let previous_project = self.project.clone();
        let mut touched = false;
        if let Some(track) = self.project.track_mut(track_id) {
            for note in track.notes.iter_mut().filter(|note| ids.contains(&note.id)) {
                note.velocity = self.active_velocity;
                touched = true;
            }
        }
        if touched {
            self.record_project_edit(previous_project);
            self.refresh_after_edit(track_id);
        }
    }

    /// Copies the effects applied to new notes onto every selected note.
    fn apply_effects_to_selected(&mut self) {
        let ids = self.selected_note_ids();
        if ids.is_empty() {
            return;
        }
        let track_id = self.selected_track_id;
        let previous_project = self.project.clone();
        let mut touched = false;
        if let Some(track) = self.project.track_mut(track_id) {
            for note in track.notes.iter_mut().filter(|note| ids.contains(&note.id)) {
                note.effects = self.active_effects.clone();
                touched = true;
            }
        }
        if touched {
            self.record_project_edit(previous_project);
            self.refresh_after_edit(track_id);
        }
    }

    /// Flips one effect in the effects applied to new notes, and sets that
    /// same effect on every selected note so other effects stay untouched.
    fn toggle_active_effect(&mut self, kind: EffectKind) {
        let enabled = {
            let flag = kind.flag(&mut self.active_effects);
            *flag = !*flag;
            *flag
        };
        let ids = self.selected_note_ids();
        if ids.is_empty() {
            return;
        }
        let track_id = self.selected_track_id;
        let previous_project = self.project.clone();
        let mut touched = false;
        if let Some(track) = self.project.track_mut(track_id) {
            for note in track.notes.iter_mut().filter(|note| ids.contains(&note.id)) {
                *kind.flag(&mut note.effects) = enabled;
                touched = true;
            }
        }
        if touched {
            self.record_project_edit(previous_project);
            self.refresh_after_edit(track_id);
        }
    }

    /// Mouse wheel zooms the score around the pointer (Ctrl+wheel and pinch
    /// do too); Shift+wheel scrolls it vertically.
    fn handle_canvas_scroll(&mut self, ui: &egui::Ui, response: &egui::Response, page: Rect) {
        let Some(track) = self.project.track(self.selected_track_id) else {
            return;
        };
        let max_scroll = self.max_scroll_y(track, page);
        if self.reveal_cursor {
            self.reveal_cursor = false;
            self.scroll_cursor_into_view(page);
        }
        if response.hovered() && matches!(self.canvas_drag, CanvasDragMode::Idle) {
            let (scroll, pinch, shift, pointer) = ui.input(|input| {
                (
                    input.smooth_scroll_delta,
                    input.zoom_delta(),
                    input.modifiers.shift,
                    input.pointer.hover_pos(),
                )
            });
            if shift {
                self.scroll_y = (self.scroll_y - (scroll.x + scroll.y)).clamp(0.0, max_scroll);
            } else {
                let factor = if (pinch - 1.0).abs() > 1e-6 {
                    pinch
                } else {
                    (scroll.y * WHEEL_ZOOM_PER_POINT).exp()
                };
                if (factor - 1.0).abs() > 1e-6 {
                    self.zoom_score_at(factor, pointer.unwrap_or(page.center()), page);
                }
            }
        }
        let max_scroll = self
            .project
            .track(self.selected_track_id)
            .map_or(0.0, |track| self.max_scroll_y(track, page));
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
    }

    fn show_editor(&mut self, ui: &mut egui::Ui) {
        let desired = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, self.theme.canvas_backdrop);

        let page = rect.shrink2(Vec2::new(18.0, 18.0));
        painter.rect_filled(page, 0.0, self.theme.sheet_background);

        self.handle_canvas_scroll(ui, &response, page);
        let track_rect = self.paint_selected_track_page(&painter, page);
        self.paint_shadow_overview(&painter, page);

        if response.drag_started() {
            // egui reports the start only after the pointer has already moved
            // a few points, so the press position is what must be hit-tested
            // and measured from; otherwise a quick flick can miss a note edge
            // and the previewed bricks would sit off the cursor.
            let press = ui
                .input(|input| input.pointer.press_origin())
                .or_else(|| response.interact_pointer_pos());
            if let Some(pos) = press {
                self.begin_canvas_drag(pos, track_rect);
            }
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
        // `PopupCloseBehavior::CloseOnClickOutside` keeps nested controls
        // usable: egui's default `CloseOnClick` closes the menu on *any*
        // click, including one on a widget inside it. Menu items that should
        // close on click (most actions) do so via their own `ui.close()`.
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
            CanvasDragMode::Stretching { .. } | CanvasDragMode::MovingNotes { .. } => {
                let snapping_disabled = ui.input(|input| input.modifiers.shift);
                if let Some(plan) = self.note_drag_plan(snapping_disabled) {
                    self.paint_note_drag_preview(&painter, track_rect, &plan);
                }
            }
            CanvasDragMode::Idle => {}
        }
    }

    /// Every note the drag would change, as `(before, after)` pairs. The edit
    /// runs on a scratch copy of the project, so clamping against neighbours
    /// and bar limits shows up in the preview exactly as it will on drop.
    fn note_drag_changes(
        &self,
        plan: &NoteDragPlan,
    ) -> Vec<(crate::core::project::TabNote, crate::core::project::TabNote)> {
        let track_id = self.selected_track_id;
        let mut scratch = self.project.clone();
        apply_note_drag(&mut scratch, track_id, plan, self.stretch_step_ticks);
        let (Some(before), Some(after)) = (self.project.track(track_id), scratch.track(track_id))
        else {
            return Vec::new();
        };
        plan.note_ids
            .iter()
            .filter_map(|id| {
                let old = before.notes.iter().find(|note| note.id == *id)?;
                let new = after.notes.iter().find(|note| note.id == *id)?;
                let changed = old.abs_tick != new.abs_tick
                    || old.string_index != new.string_index
                    || old.duration_ticks != new.duration_ticks;
                changed.then(|| (old.clone(), new.clone()))
            })
            .collect()
    }

    /// The visible piece of `note` in each row it touches, as
    /// `(rect, is_onset, continues_into_next_row)`.
    fn brick_fragments(
        &self,
        geometry: &PageGeometry,
        note: &crate::core::project::TabNote,
    ) -> Vec<(Rect, bool, bool)> {
        let note_end = note.end_tick();
        geometry
            .systems
            .iter()
            .enumerate()
            .filter_map(|(index, system)| {
                let origin = system.start_tick;
                let system_end = origin + system.tick_span();
                if note_end <= origin || note.abs_tick >= system_end {
                    return None;
                }
                let fragment_start = note.abs_tick.max(origin);
                let fragment_end = note_end.min(system_end);
                let is_onset = note.abs_tick >= origin;
                let tab_y = geometry.system_top(index) + geometry.metrics.tab_offset;
                let right = (geometry.left + system.right_x(self.pixels_per_tick))
                    .min(geometry.right);
                let rect = self.brick_fragment_rect(
                    geometry.left,
                    tab_y,
                    origin,
                    right,
                    note,
                    fragment_start,
                    fragment_end,
                    is_onset,
                );
                Some((rect, is_onset, fragment_end < note_end))
            })
            .collect()
    }

    /// Live preview of a move or stretch: the notes' old places are dimmed,
    /// and the bricks as they will land are drawn translucent on top.
    fn paint_note_drag_preview(&self, painter: &egui::Painter, page: Rect, plan: &NoteDragPlan) {
        let Some(track) = self.project.track(self.selected_track_id) else {
            return;
        };
        let painter = painter.with_clip_rect(page);
        let geometry = self.page_geometry(track, page);
        let changes = self.note_drag_changes(plan);
        let accent = self.theme.accent_color;
        let background = self.theme.sheet_background;
        let veil = egui::Color32::from_rgba_unmultiplied(
            background.r(),
            background.g(),
            background.b(),
            190,
        );
        let ghost_fill =
            egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 165);
        let ghost_stroke = Stroke::new(1.5_f32, accent);

        for (before, after) in &changes {
            for (rect, _, _) in self.brick_fragments(&geometry, before) {
                painter.rect_filled(rect.expand(1.0), 1.0, veil);
                let outline = rect.expand(0.5);
                painter.extend(egui::Shape::dashed_line(
                    &[
                        outline.left_top(),
                        outline.right_top(),
                        outline.right_bottom(),
                        outline.left_bottom(),
                        outline.left_top(),
                    ],
                    Stroke::new(1.0_f32, accent.gamma_multiply(0.8)),
                    3.0,
                    2.0,
                ));
            }
            for (rect, is_onset, continues) in self.brick_fragments(&geometry, after) {
                paint_note_block(&painter, rect, ghost_fill, ghost_stroke, !is_onset, continues);
                if is_onset {
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        after.fret.to_string(),
                        FontId::monospace(if rect.width() < 11.0 { 7.0 } else { 9.0 }),
                        egui::Color32::WHITE,
                    );
                }
            }
        }

        let caption = match (changes.first(), &plan.edit) {
            (None, _) => "No change".to_owned(),
            (Some((before, after)), NoteDragEdit::Move { .. }) => {
                let ticks = after.abs_tick - before.abs_tick;
                let strings = after.string_index as i32 - before.string_index as i32;
                format!(
                    "{ticks:+} ticks{}{}",
                    if strings == 0 {
                        String::new()
                    } else {
                        format!(", string {strings:+}")
                    },
                    if plan.snapping_disabled { " (no snap)" } else { "" }
                )
            }
            (Some((before, after)), NoteDragEdit::Stretch { edge, .. }) => match edge {
                StretchEdge::Right => {
                    format!("{:+} ticks", after.duration_ticks - before.duration_ticks)
                }
                StretchEdge::Left => format!("{:+} ticks", after.abs_tick - before.abs_tick),
            },
        };
        let caption = if changes.len() > 1 {
            format!("{} notes: {caption}", changes.len())
        } else {
            caption
        };
        let galley = painter.layout_no_wrap(caption, FontId::monospace(11.0), egui::Color32::WHITE);
        let mut top_left = plan.pointer + Vec2::new(14.0, -24.0);
        top_left.x = top_left.x.min(page.right() - galley.size().x - 10.0).max(page.left());
        top_left.y = top_left.y.max(page.top() + 2.0);
        painter.rect_filled(
            Rect::from_min_size(top_left, galley.size()).expand2(Vec2::new(5.0, 2.0)),
            3.0,
            egui::Color32::from_rgba_unmultiplied(20, 22, 25, 225),
        );
        painter.galley(top_left, galley, egui::Color32::WHITE);
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
                self.show_time_signature_picker(ui);
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
                    ui.separator();
                    for (dots, label) in [(1_u8, "Dotted"), (2_u8, "Double dotted")] {
                        if ui
                            .selectable_label(self.selected_dots == dots, label)
                            .clicked()
                        {
                            self.selected_dots = if self.selected_dots == dots { 0 } else { dots };
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
            if ui
                .add_enabled(
                    has_selection || has_area_selection,
                    egui::Button::new("Tie Note  (L)"),
                )
                .clicked()
            {
                self.toggle_tie_on_selection();
                ui.close();
            }
            for label in [
                "Staccatissimo",
                "Tenuto",
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

    fn page_geometry(&self, track: &InstrumentTrack, page: Rect) -> PageGeometry {
        let left = page.left() + PAGE_LEFT_INSET;
        let right = page.right() - PAGE_RIGHT_INSET;
        PageGeometry {
            left,
            right,
            first_top: page.top() + PAGE_TOP_INSET - self.scroll_y,
            systems: self.layout_staff_systems(track, (right - left).max(40.0)),
            metrics: self.system_metrics(track),
        }
    }

    /// The farthest the score can scroll before its last row leaves the page.
    fn max_scroll_y(&self, track: &InstrumentTrack, page: Rect) -> f32 {
        let geometry = self.page_geometry(track, page);
        let content =
            PAGE_TOP_INSET + geometry.systems.len() as f32 * geometry.metrics.height;
        (content + PAGE_BOTTOM_PADDING - page.height()).max(0.0)
    }

    /// Zooms the score by `factor` while keeping the staff row under `anchor`
    /// at the same screen height. Rows reflow to the page width at every
    /// zoom level, so only the vertical position can be held in place.
    fn zoom_score_at(&mut self, factor: f32, anchor: Pos2, page: Rect) {
        let new_scale =
            (self.pixels_per_tick * factor).clamp(MIN_PIXELS_PER_TICK, MAX_PIXELS_PER_TICK);
        if (new_scale - self.pixels_per_tick).abs() < f32::EPSILON {
            return;
        }
        let Some(track) = self.project.track(self.selected_track_id) else {
            return;
        };
        let before = self.page_geometry(track, page);
        let row = before.system_index_at(anchor.y);
        let Some(system) = before.systems.get(row) else {
            return;
        };
        let tick = system.tick_at(anchor.x - before.left, self.pixels_per_tick);
        let offset_in_row = anchor.y - before.system_top(row);

        self.pixels_per_tick = new_scale;
        let Some(track) = self.project.track(self.selected_track_id) else {
            return;
        };
        let after = self.page_geometry(track, page);
        let new_row = after.system_index_of_tick(tick);
        let drift = after.system_top(new_row) - (anchor.y - offset_in_row);
        let max_scroll = self.max_scroll_y(track, page);
        self.scroll_y = (self.scroll_y + drift).clamp(0.0, max_scroll);
    }

    /// Scrolls just far enough to bring the edit cursor's row onto the page.
    fn scroll_cursor_into_view(&mut self, page: Rect) {
        let Some(track) = self.project.track(self.selected_track_id) else {
            return;
        };
        let geometry = self.page_geometry(track, page);
        let row = geometry.system_index_of_tick(self.cursor.tick);
        let top = PAGE_TOP_INSET + row as f32 * geometry.metrics.height;
        let bottom = top + geometry.metrics.height;
        let mut scroll = self.scroll_y;
        if top < scroll + PAGE_TOP_INSET {
            scroll = (top - PAGE_TOP_INSET).max(0.0);
        } else if bottom > scroll + page.height() - PAGE_BOTTOM_PADDING {
            scroll = bottom - page.height() + PAGE_BOTTOM_PADDING;
        }
        self.scroll_y = scroll.clamp(0.0, self.max_scroll_y(track, page));
    }

    /// Lays a track's bars out into rows ("systems"): each system is a
    /// contiguous run of whole bars that fits within `available_width`. A
    /// bar is never split across two systems - if the next bar wouldn't fit
    /// in the remaining space of the current row, the whole row ends there
    /// and that bar starts the next one, mirroring how real notation
    /// software wraps.
    fn layout_staff_systems(
        &self,
        track: &InstrumentTrack,
        available_width: f32,
    ) -> Vec<StaffSystem> {
        match self.view_mode {
            EditorViewMode::Bricks => self.layout_brick_systems(track, available_width),
            EditorViewMode::TabNumbers => self.layout_beat_systems(track, available_width),
        }
    }

    /// The rest policy used wherever rests are shown in tab view and in the
    /// classic staff: every gap is filled, in slots of the duration being
    /// entered, as TuxGuitar does.
    fn tab_rest_fill(&self) -> RestFill {
        RestFill::AllGaps {
            preferred: self.entry_ticks(),
        }
    }

    /// Onset ticks of every beat in a bar: note onsets and rest onsets, plus
    /// the bar start. These are the positions the tab cursor can stop on.
    fn bar_beat_ticks(&self, track: &InstrumentTrack, bar: (Tick, Tick)) -> Vec<Tick> {
        let mut ticks: Vec<Tick> = vec![bar.0];
        ticks.extend(
            track
                .notes
                .iter()
                .filter(|note| note.abs_tick >= bar.0 && note.abs_tick < bar.1)
                .map(|note| note.abs_tick),
        );
        ticks.extend(
            self.project
                .bar_rests(track.id, bar.0, self.tab_rest_fill())
                .iter()
                .map(|rest| rest.abs_tick),
        );
        ticks.sort_unstable();
        ticks.dedup();
        ticks
    }

    /// Width of the slot a beat occupies, from the time until the next beat.
    /// Longer notes get more room, but sub-linearly, so a whole rest doesn't
    /// swallow the row while sixteenths stay readable.
    fn beat_slot_width(&self, ticks_until_next: Tick) -> f32 {
        let zoom = (self.pixels_per_tick / 0.07).clamp(0.5, 2.0);
        let quarters = ticks_until_next.max(1) as f32 / TICKS_PER_QUARTER as f32;
        let natural = 12.0 + 32.0 * quarters.powf(0.6) * zoom;
        let minimum = if self.show_classic_notation {
            22.0
        } else {
            16.0
        };
        natural.max(minimum)
    }

    /// Tab-view layout: bars are packed into rows by their natural width,
    /// then each full row is stretched to the page width, as in
    /// TuxGuitar and Guitar Pro.
    fn layout_beat_systems(
        &self,
        track: &InstrumentTrack,
        available_width: f32,
    ) -> Vec<StaffSystem> {
        const BAR_LEAD: f32 = 4.0;
        // Extra room after the barline for a time signature label.
        const SIGNATURE_LEAD: f32 = 24.0;
        const COLUMN_LEAD: f32 = 8.0;
        const BAR_TRAIL: f32 = 2.0;

        struct NaturalBar {
            bounds: (Tick, Tick),
            lead: f32,
            beats: Vec<Tick>,
            slots: Vec<f32>,
        }

        let mut bars: Vec<NaturalBar> = Vec::new();
        let mut cursor: Tick = 0;
        for _ in 0..track.bar_count.max(1) {
            let bounds = track.measure_bounds_at(cursor);
            cursor = bounds.1;
            let beats = self.bar_beat_ticks(track, bounds);
            let slots = beats
                .iter()
                .enumerate()
                .map(|(index, tick)| {
                    let next = beats.get(index + 1).copied().unwrap_or(bounds.1);
                    self.beat_slot_width(next - tick)
                })
                .collect();
            let lead = if signature_label_at(track, bounds.0) {
                SIGNATURE_LEAD
            } else {
                BAR_LEAD
            };
            bars.push(NaturalBar {
                bounds,
                lead,
                beats,
                slots,
            });
        }

        let natural_width = |bar: &NaturalBar| bar.lead + bar.slots.iter().sum::<f32>() + BAR_TRAIL;

        let mut rows: Vec<Vec<NaturalBar>> = Vec::new();
        let mut current: Vec<NaturalBar> = Vec::new();
        let mut current_width = 0.0_f32;
        for bar in bars {
            let width = natural_width(&bar);
            if !current.is_empty() && current_width + width > available_width {
                rows.push(std::mem::take(&mut current));
                current_width = 0.0;
            }
            current_width += width;
            current.push(bar);
        }
        if !current.is_empty() {
            rows.push(current);
        }
        if rows.is_empty() {
            return vec![StaffSystem::from_bars(Vec::new())];
        }

        let row_count = rows.len();
        rows.into_iter()
            .enumerate()
            .map(|(row_index, row)| {
                let fixed: f32 = row.iter().map(|bar| bar.lead + BAR_TRAIL).sum();
                let slot_total: f32 = row.iter().flat_map(|bar| bar.slots.iter()).sum();
                let is_last_row = row_index + 1 == row_count;
                // Full rows are stretched to the page width; the last row
                // keeps its natural width.
                let stretch = if is_last_row || slot_total <= 0.0 {
                    1.0
                } else {
                    ((available_width - fixed) / slot_total).max(1.0)
                };

                let mut system = StaffSystem::from_bars(row.iter().map(|bar| bar.bounds).collect());
                let mut x = 0.0_f32;
                for bar in &row {
                    let slots: Vec<f32> = bar.slots.iter().map(|slot| slot * stretch).collect();
                    let width = bar.lead + slots.iter().sum::<f32>() + BAR_TRAIL;
                    let mut slot_start = x + bar.lead;
                    let mut columns = Vec::with_capacity(bar.beats.len());
                    for (tick, slot) in bar.beats.iter().zip(&slots) {
                        columns.push((*tick, slot_start + COLUMN_LEAD.min(slot * 0.5)));
                        slot_start += slot;
                    }
                    system.beat_bars.push(BeatBarLayout {
                        start: bar.bounds.0,
                        end: bar.bounds.1,
                        x,
                        width,
                        columns,
                    });
                    x += width;
                }
                system
            })
            .collect()
    }

    fn layout_brick_systems(
        &self,
        track: &InstrumentTrack,
        available_width: f32,
    ) -> Vec<StaffSystem> {
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
            systems.push(StaffSystem::from_bars(Vec::new()));
        }
        systems
    }

    /// Draws a "♩=bpm" label above the staff at each non-hidden tempo point
    /// falling within `[system_origin_tick, system_origin_tick+system_ticks)`,
    /// positioned right above the point's tick - matching Guitar Pro's
    /// automation display.
    fn paint_tempo_labels(
        &self,
        painter: &egui::Painter,
        left: f32,
        staff_y: f32,
        system: &StaffSystem,
    ) {
        let system_origin_tick = system.start_tick;
        let system_ticks = system.tick_span();
        for point in self.project.tempo_points.iter().filter(|point| {
            !point.hidden
                && point.at_tick >= system_origin_tick
                && point.at_tick < system_origin_tick + system_ticks
        }) {
            let x = left + system.x_of(point.at_tick, self.pixels_per_tick);
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
        let PageGeometry {
            left,
            right,
            first_top,
            systems,
            metrics,
        } = self.page_geometry(track, page);
        let clef = Clef::for_track(track);
        // Rows scrolled past the page edge must not draw into its margin.
        let painter = &painter.with_clip_rect(page);

        painter.text(
            Pos2::new(page.left() + 16.0, page.top() + 12.0 - self.scroll_y),
            Align2::LEFT_TOP,
            format!("{} - {}", self.project.title, track.name),
            FontId::proportional(15.0),
            self.theme.notation_foreground,
        );

        for (system_index, system) in systems.iter().enumerate() {
            let top = first_top + system_index as f32 * metrics.height;
            if top > page.bottom() {
                break;
            }
            if top + metrics.height < page.top() {
                continue;
            }
            let staff_y = top + metrics.staff_offset;
            let tab_y = top + metrics.tab_offset;
            let tab_bottom = tab_y + metrics.tab_span();
            // Bar lines, tempo labels and the system's top edge start at the
            // classic staff when it's shown, otherwise at the first string.
            let system_top = if self.show_classic_notation {
                staff_y
            } else {
                tab_y
            };
            let system_end_tick = system.start_tick + system.tick_span();
            // A row only extends as far as its bars reach. Brick rows are
            // also clipped to the page; beat rows are already fitted to it.
            let row_right = left + system.right_x(self.pixels_per_tick);
            let system_right = if system.has_beat_layout() {
                row_right
            } else {
                row_right.min(right)
            };

            if self.show_classic_notation {
                for line in 0..5 {
                    let y = staff_y + line as f32 * SystemMetrics::STAFF_LINE_GAP;
                    painter.line_segment(
                        [Pos2::new(left, y), Pos2::new(system_right, y)],
                        self.theme.staff_stroke(),
                    );
                }
                painter.text(
                    Pos2::new(left - 26.0, staff_y + 14.0),
                    Align2::CENTER_CENTER,
                    if clef == Clef::Bass { "F" } else { "G" },
                    FontId::proportional(28.0),
                    self.theme.notation_foreground,
                );
                // Guitars and basses sound an octave below the written pitch.
                painter.text(
                    Pos2::new(left - 26.0, staff_y + 33.0),
                    Align2::CENTER_CENTER,
                    "8",
                    FontId::proportional(10.0),
                    self.theme.notation_foreground,
                );
            }
            for string in 0..track.tuning.len() {
                let y = tab_y + string as f32 * SystemMetrics::STRING_GAP;
                painter.line_segment(
                    [Pos2::new(left, y), Pos2::new(system_right, y)],
                    self.theme.staff_stroke(),
                );
            }

            // Tempo labels clear the tallest stems above the staff.
            let label_baseline = if self.show_classic_notation {
                top + 20.0
            } else {
                tab_y
            };
            self.paint_tempo_labels(painter, left, label_baseline, system);

            let signature_y = if self.show_classic_notation {
                staff_y + 14.0
            } else {
                tab_y + metrics.tab_span() * 0.5
            };
            for (bar_index, &(bar_start, _bar_end)) in system.bars.iter().enumerate() {
                let x = left + system.bar_x(bar_index, self.pixels_per_tick);
                painter.line_segment(
                    [Pos2::new(x, system_top), Pos2::new(x, tab_bottom + 3.0)],
                    self.theme.bar_stroke(),
                );
                // Only label a bar where the signature actually *changes*
                // (or the very first bar of the piece) - not every bar that
                // happens to share the current signature.
                let sig = track.signature_at(bar_start);
                if signature_label_at(track, bar_start) {
                    // Brick view has no spare room after the barline (a note
                    // may start right on it), so the opening signature goes
                    // in front of the staff.
                    let label_x = if bar_index == 0 && !system.has_beat_layout() {
                        x - 12.0
                    } else {
                        x + 6.0
                    };
                    painter.text(
                        Pos2::new(label_x, signature_y),
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
                    Pos2::new(system_right, system_top),
                    Pos2::new(system_right, tab_bottom + 3.0),
                ],
                self.theme.bar_stroke(),
            );

            self.paint_cursor_cell(painter, left, tab_y, system);

            if self.show_classic_notation {
                self.paint_classic_notation(painter, track, clef, system, left, staff_y);
            }

            let system_origin_tick = system.start_tick;
            match self.view_mode {
                EditorViewMode::Bricks => {
                    // Continuation fragments first: bricks whose onset was
                    // in an earlier row but that still sustain into this
                    // one. Drawn with no left border so they read as one
                    // seamless block rather than a new note.
                    for note in track.notes.iter().filter(|note| {
                        note.abs_tick < system_origin_tick
                            && note.abs_tick + note.duration_ticks > system_origin_tick
                    }) {
                        let fragment_end =
                            (note.abs_tick + note.duration_ticks).min(system_end_tick);
                        self.paint_note_fragment(
                            painter,
                            left,
                            tab_y,
                            system_origin_tick,
                            system_right,
                            note,
                            system_origin_tick,
                            fragment_end,
                            false,
                        );
                    }

                    // Starting fragments: notes whose onset falls in this
                    // row. A brick reaching past the row's right wall is
                    // drawn only up to the wall (no right border, since it
                    // continues as a continuation fragment in a later row).
                    for note in track.notes.iter().filter(|note| {
                        note.abs_tick >= system_origin_tick && note.abs_tick < system_end_tick
                    }) {
                        let fragment_end =
                            (note.abs_tick + note.duration_ticks).min(system_end_tick);
                        self.paint_note_fragment(
                            painter,
                            left,
                            tab_y,
                            system_origin_tick,
                            system_right,
                            note,
                            note.abs_tick,
                            fragment_end,
                            true,
                        );
                    }
                }
                EditorViewMode::TabNumbers => {
                    for note in track.notes.iter().filter(|note| {
                        note.abs_tick >= system_origin_tick && note.abs_tick < system_end_tick
                    }) {
                        self.paint_tab_number(painter, track, left, tab_y, system, note);
                    }
                }
            }

            if self.tab_rhythm_shown() {
                self.paint_tab_rhythm(painter, track, clef, system, left, tab_bottom);
            }

            // Rests: every gap in tab view, only the end of each bar in
            // brick view (where a gap between bricks is simply empty).
            for (bar_index, &(bar_start, bar_end)) in system.bars.iter().enumerate() {
                match self.view_mode {
                    EditorViewMode::Bricks => {
                        for rest in self.project.tail_padding_rests(track.id, bar_start) {
                            let x = left + system.x_of(rest.abs_tick, self.pixels_per_tick);
                            let y = tab_bottom + 17.0;
                            painter.line_segment(
                                [Pos2::new(x, y), Pos2::new(x + 14.0, y)],
                                Stroke::new(2.0, self.theme.rest_color),
                            );
                        }
                    }
                    EditorViewMode::TabNumbers => {
                        for rest in
                            self.project
                                .bar_rests(track.id, bar_start, self.tab_rest_fill())
                        {
                            let (cx, value) =
                                self.rest_anchor(system, left, bar_index, bar_end, &rest);
                            paint_rest_symbol(
                                painter,
                                cx,
                                tab_y + metrics.tab_span() * 0.5,
                                value,
                                self.theme.rest_color,
                            );
                        }
                    }
                }
            }
        }

        page
    }

    /// Where to draw a rest and which note value its glyph shows. A rest
    /// covering a whole bar is a whole rest centered in the bar, whatever
    /// the bar's length.
    fn rest_anchor(
        &self,
        system: &StaffSystem,
        left: f32,
        bar_index: usize,
        bar_end: Tick,
        rest: &crate::core::project::RestNode,
    ) -> (f32, notation::NoteValue) {
        let bar_start = system.bars.get(bar_index).map(|bar| bar.0).unwrap_or(0);
        if rest.abs_tick == bar_start && rest.abs_tick + rest.duration_ticks == bar_end {
            let x0 = left + system.bar_x(bar_index, self.pixels_per_tick);
            let x1 = match system.beat_bars.get(bar_index) {
                Some(bar) => left + bar.x + bar.width,
                None => left + system.x_of(bar_end, self.pixels_per_tick),
            };
            return ((x0 + x1) * 0.5, notation::NoteValue { value: 1, dots: 0 });
        }
        (
            self.content_x(system, left, rest.abs_tick),
            notation::note_value_for(rest.duration_ticks),
        )
    }

    fn system_metrics(&self, track: &InstrumentTrack) -> SystemMetrics {
        SystemMetrics::new(self.show_classic_notation, track.tuning.len())
    }

    /// Distance from the first to the last tab string of the selected track.
    fn selected_string_span(&self) -> f32 {
        self.project
            .track(self.selected_track_id)
            .map(|track| self.system_metrics(track).tab_span())
            .unwrap_or(0.0)
    }

    fn is_note_selected(&self, note_id: u64) -> bool {
        self.selected_note_id == Some(note_id) || self.area_selected_note_ids.contains(&note_id)
    }

    /// Canvas x of the center of whatever is drawn at `tick`: the beat's
    /// column in tab view, or the onset plus a small offset in brick view
    /// (so a number at tick 0 clears the barline).
    fn content_x(&self, system: &StaffSystem, left: f32, tick: Tick) -> f32 {
        let offset = if system.has_beat_layout() {
            0.0
        } else {
            TAB_NUMBER_CENTER_OFFSET
        };
        left + system.x_of(tick, self.pixels_per_tick) + offset
    }

    /// Draws the notes and rests of every bar in `system` on the classic
    /// staff: heads by real pitch and duration, ledger lines, accidentals,
    /// dots, stems, flags, beams and ties.
    fn paint_classic_notation(
        &self,
        painter: &egui::Painter,
        track: &InstrumentTrack,
        clef: Clef,
        system: &StaffSystem,
        left: f32,
        staff_y: f32,
    ) {
        use notation::{Accidental, ScoreItem};

        const STEM_LENGTH: f32 = 24.0;
        let step_px = SystemMetrics::STAFF_LINE_GAP / 2.0;
        let bottom_y = staff_y + SystemMetrics::STAFF_HEIGHT;
        let y_of = |step: i32| bottom_y - step as f32 * step_px;
        let ink = self.theme.notation_foreground;
        let thin = Stroke::new(1.0_f32, ink);
        let stem_stroke = Stroke::new(1.2_f32, ink);

        // Head centers by note id, for drawing ties afterwards.
        let mut head_spots: std::collections::HashMap<u64, (f32, f32, bool)> =
            std::collections::HashMap::new();
        let mut ties: Vec<(u64, u64)> = Vec::new();

        for (bar_index, &(bar_start, bar_end)) in system.bars.iter().enumerate() {
            let signature = track.signature_at(bar_start);
            let rests = self
                .project
                .bar_rests(track.id, bar_start, self.tab_rest_fill());
            let score = notation::score_bar(
                track,
                clef,
                bar_start,
                bar_end,
                &rests,
                notation::beat_ticks_for(signature.numerator as u32, signature.denominator as u32),
            );

            // Flat beam tip for each beamed chord: far enough from the
            // outermost head of the group.
            let mut beam_tips: std::collections::HashMap<usize, f32> =
                std::collections::HashMap::new();
            for group in &score.beams {
                let mut tip = if group.stem_up { f32::MAX } else { f32::MIN };
                for &position in &group.items {
                    if let ScoreItem::Chord(chord) = &score.items[position] {
                        if group.stem_up {
                            tip = tip.min(y_of(chord.highest_step()) - STEM_LENGTH);
                        } else {
                            tip = tip.max(y_of(chord.lowest_step()) + STEM_LENGTH);
                        }
                    }
                }
                // A stem always reaches at least the middle line.
                tip = if group.stem_up {
                    tip.min(y_of(4))
                } else {
                    tip.max(y_of(4))
                };
                for &position in &group.items {
                    beam_tips.insert(position, tip);
                }
            }

            let mut stem_xs: std::collections::HashMap<usize, f32> =
                std::collections::HashMap::new();

            for (position, item) in score.items.iter().enumerate() {
                match item {
                    ScoreItem::Rest { tick, value } => {
                        let rest = crate::core::project::RestNode {
                            abs_tick: *tick,
                            duration_ticks: 0,
                        };
                        let whole_bar = rests.iter().any(|candidate| {
                            candidate.abs_tick == *tick
                                && candidate.abs_tick == bar_start
                                && candidate.abs_tick + candidate.duration_ticks == bar_end
                        });
                        let (cx, glyph) = if whole_bar {
                            self.rest_anchor(
                                system,
                                left,
                                bar_index,
                                bar_end,
                                &crate::core::project::RestNode {
                                    abs_tick: bar_start,
                                    duration_ticks: bar_end - bar_start,
                                },
                            )
                        } else {
                            (self.content_x(system, left, rest.abs_tick), *value)
                        };
                        let reference = match glyph.value {
                            1 => y_of(6),
                            _ => y_of(4),
                        };
                        paint_rest_symbol(painter, cx, reference, glyph, self.theme.rest_color);
                    }
                    ScoreItem::Chord(chord) => {
                        let cx = self.content_x(system, left, chord.tick);
                        let mut heads = chord.heads.clone();
                        heads.sort_by_key(|head| head.step);

                        // Heads a second apart would overlap: the upper
                        // one moves to the other side of the stem.
                        let mut displaced = vec![false; heads.len()];
                        for index in 1..heads.len() {
                            if heads[index].step - heads[index - 1].step == 1
                                && !displaced[index - 1]
                            {
                                displaced[index] = true;
                            }
                        }
                        let side = if chord.stem_up { 1.0 } else { -1.0 };

                        // Accidentals sit left of the heads, stepping
                        // further left when two would collide.
                        let mut accidental_column = 0;
                        let mut last_accidental_step = i32::MIN;
                        for head in heads.iter().rev() {
                            if let Some(accidental) = head.accidental {
                                if last_accidental_step != i32::MIN
                                    && last_accidental_step - head.step < 5
                                {
                                    accidental_column += 1;
                                } else {
                                    accidental_column = 0;
                                }
                                last_accidental_step = head.step;
                                let ax = cx - 10.0 - accidental_column as f32 * 7.0;
                                paint_accidental(
                                    painter,
                                    ax,
                                    y_of(head.step),
                                    accidental == Accidental::Sharp,
                                    ink,
                                );
                            }
                        }

                        for (head, &moved) in heads.iter().zip(&displaced) {
                            let hx = cx + if moved { side * 8.4 } else { 0.0 };
                            let hy = y_of(head.step);
                            let selected = self.is_note_selected(head.note_id);
                            let color = if selected {
                                self.theme.accent_color
                            } else {
                                ink
                            };

                            // Ledger lines below and above the staff.
                            let ledger = |step: i32| {
                                painter.line_segment(
                                    [
                                        Pos2::new(hx - 7.5, y_of(step)),
                                        Pos2::new(hx + 7.5, y_of(step)),
                                    ],
                                    thin,
                                );
                            };
                            if head.step < 0 {
                                let mut step = -2;
                                while step >= head.step {
                                    ledger(step);
                                    step -= 2;
                                }
                            } else if head.step > 8 {
                                let mut step = 10;
                                while step <= head.step {
                                    ledger(step);
                                    step += 2;
                                }
                            }

                            if chord.value.value == 1 {
                                painter.add(egui::epaint::EllipseShape::stroke(
                                    Pos2::new(hx, hy),
                                    Vec2::new(5.4, 3.5),
                                    Stroke::new(1.4_f32, color),
                                ));
                            } else if chord.value.filled_head() {
                                painter.add(egui::epaint::EllipseShape::filled(
                                    Pos2::new(hx, hy),
                                    Vec2::new(4.6, 3.4),
                                    color,
                                ));
                            } else {
                                painter.add(egui::epaint::EllipseShape::stroke(
                                    Pos2::new(hx, hy),
                                    Vec2::new(4.3, 3.1),
                                    Stroke::new(1.3_f32, color),
                                ));
                            }
                            head_spots.insert(head.note_id, (hx, hy, chord.stem_up));
                        }

                        // Augmentation dots go in the space to the right of
                        // the heads, nudged off a line.
                        for head in &heads {
                            for dot in 0..chord.value.dots {
                                let dy = if head.step % 2 == 0 { -step_px } else { 0.0 };
                                painter.circle_filled(
                                    Pos2::new(
                                        cx + 9.5
                                            + dot as f32 * 4.0
                                            + if displaced.iter().any(|d| *d) && chord.stem_up {
                                                8.4
                                            } else {
                                                0.0
                                            },
                                        y_of(head.step) + dy,
                                    ),
                                    1.3,
                                    ink,
                                );
                            }
                        }

                        if chord.value.has_stem() {
                            let stem_x = if chord.stem_up { cx + 4.4 } else { cx - 4.4 };
                            stem_xs.insert(position, stem_x);
                            let tip = beam_tips.get(&position).copied().unwrap_or_else(|| {
                                if chord.stem_up {
                                    (y_of(chord.highest_step()) - STEM_LENGTH).min(y_of(4))
                                } else {
                                    (y_of(chord.lowest_step()) + STEM_LENGTH).max(y_of(4))
                                }
                            });
                            let base = if chord.stem_up {
                                y_of(chord.lowest_step())
                            } else {
                                y_of(chord.highest_step())
                            };
                            painter.line_segment(
                                [Pos2::new(stem_x, base), Pos2::new(stem_x, tip)],
                                stem_stroke,
                            );

                            // Unbeamed flagged notes get flags on the stem.
                            if chord.value.flags() > 0 && !beam_tips.contains_key(&position) {
                                for flag in 0..chord.value.flags() {
                                    let dy = flag as f32 * 5.0;
                                    let (y0, y1, y2) = if chord.stem_up {
                                        (tip + dy, tip + dy + 5.0, tip + dy + 11.0)
                                    } else {
                                        (tip - dy, tip - dy - 5.0, tip - dy - 11.0)
                                    };
                                    painter.add(egui::Shape::line(
                                        vec![
                                            Pos2::new(stem_x, y0),
                                            Pos2::new(stem_x + 4.0, y1),
                                            Pos2::new(stem_x + 6.0, y2),
                                        ],
                                        Stroke::new(1.5_f32, ink),
                                    ));
                                }
                            }
                        }

                        for note in track.notes.iter().filter(|note| {
                            note.abs_tick == chord.tick && track.is_tied_continuation(note)
                        }) {
                            if let Some(previous) = track.tie_predecessor(note) {
                                ties.push((previous.id, note.id));
                            }
                        }
                    }
                }
            }

            // Beams: one thick bar per flag level across each group, with a
            // short stub where a single note has more flags than its
            // neighbors.
            for group in &score.beams {
                let Some(&tip) = beam_tips.get(&group.items[0]) else {
                    continue;
                };
                let levels = group
                    .items
                    .iter()
                    .filter_map(|position| match &score.items[*position] {
                        ScoreItem::Chord(chord) => Some(chord.value.flags()),
                        ScoreItem::Rest { .. } => None,
                    })
                    .max()
                    .unwrap_or(1);
                let direction = if group.stem_up { 1.0 } else { -1.0 };
                for level in 1..=levels {
                    let y = tip + direction * (level - 1) as f32 * 4.6;
                    let mut run_start: Option<usize> = None;
                    let flags_at = |index: usize| match &score.items[group.items[index]] {
                        ScoreItem::Chord(chord) => chord.value.flags(),
                        ScoreItem::Rest { .. } => 0,
                    };
                    let mut runs: Vec<(usize, usize)> = Vec::new();
                    for index in 0..group.items.len() {
                        if flags_at(index) >= level {
                            run_start.get_or_insert(index);
                        } else if let Some(start) = run_start.take() {
                            runs.push((start, index - 1));
                        }
                    }
                    if let Some(start) = run_start {
                        runs.push((start, group.items.len() - 1));
                    }
                    for (first, last) in runs {
                        let x0 = stem_xs[&group.items[first]];
                        let mut x1 = stem_xs[&group.items[last]];
                        let mut x0 = x0;
                        if first == last {
                            // Stub toward the neighbor.
                            if last + 1 < group.items.len() {
                                x1 = x0 + 7.0;
                            } else {
                                x0 -= 7.0;
                            }
                        }
                        painter.line_segment(
                            [Pos2::new(x0, y), Pos2::new(x1, y)],
                            Stroke::new(2.8_f32, ink),
                        );
                    }
                }
            }
        }

        for (from_id, to_id) in ties {
            let Some(&(x1, y1, stem_up)) = head_spots.get(&to_id) else {
                continue;
            };
            let below = stem_up;
            let start = match head_spots.get(&from_id) {
                Some(&(x0, y0, _)) => Pos2::new(x0 + 5.0, y0 + if below { 4.0 } else { -4.0 }),
                None => Pos2::new(x1 - 16.0, y1 + if below { 4.0 } else { -4.0 }),
            };
            paint_tie_arc(
                painter,
                start,
                Pos2::new(x1 - 5.0, y1 + if below { 4.0 } else { -4.0 }),
                ink,
                !below,
            );
        }
    }

    /// Draws a note as a bare fret number on its string (tab view). A tied
    /// continuation is written "(fret)" with a tie arc back to the note it
    /// continues; when that note sits in an earlier row the arc starts at
    /// the left edge of this one.
    /// Whether rhythm marks hang under the tab strings: in tab view the
    /// strings carry only fret numbers, so without the staff above them the
    /// marks are the only place a note's length shows.
    fn tab_rhythm_shown(&self) -> bool {
        self.view_mode == EditorViewMode::TabNumbers && !self.show_classic_notation
    }

    /// Distance from the last tab string to the "P.M." label, clear of the
    /// rhythm marks when they are shown.
    fn palm_mute_label_gap(&self) -> f32 {
        if self.tab_rhythm_shown() { 24.0 } else { 13.0 }
    }

    /// TuxGuitar-style rhythm under the tab strings: a stem per beat, flags
    /// or beams for short values, and a dot after the stem for each
    /// augmentation dot, all read from the notes' lengths.
    fn paint_tab_rhythm(
        &self,
        painter: &egui::Painter,
        track: &InstrumentTrack,
        clef: Clef,
        system: &StaffSystem,
        left: f32,
        tab_bottom: f32,
    ) {
        use notation::ScoreItem;

        let top = tab_bottom + 6.0;
        let tip = top + 14.0;
        let ink = self.theme.notation_foreground;
        let beam_stroke = Stroke::new(2.0_f32, ink);

        for &(bar_start, bar_end) in &system.bars {
            let signature = track.signature_at(bar_start);
            let rests = self
                .project
                .bar_rests(track.id, bar_start, self.tab_rest_fill());
            let score = notation::score_bar(
                track,
                clef,
                bar_start,
                bar_end,
                &rests,
                notation::beat_ticks_for(signature.numerator as u32, signature.denominator as u32),
            );
            let stem_x = |position: usize| {
                self.content_x(system, left, score.items[position].tick())
            };

            let mut beamed: Vec<usize> = Vec::new();
            for group in &score.beams {
                let flags: Vec<(usize, u32)> = group
                    .items
                    .iter()
                    .filter_map(|&position| match &score.items[position] {
                        ScoreItem::Chord(chord) => Some((position, chord.value.flags())),
                        ScoreItem::Rest { .. } => None,
                    })
                    .collect();
                let deepest = flags.iter().map(|(_, count)| *count).max().unwrap_or(0);
                for level in 1..=deepest {
                    let y = tip - (level - 1) as f32 * 3.5;
                    let mut index = 0;
                    while index < flags.len() {
                        if flags[index].1 < level {
                            index += 1;
                            continue;
                        }
                        let run_start = index;
                        while index < flags.len() && flags[index].1 >= level {
                            index += 1;
                        }
                        let first = stem_x(flags[run_start].0);
                        if index - run_start >= 2 {
                            let last = stem_x(flags[index - 1].0);
                            painter.line_segment(
                                [Pos2::new(first, y), Pos2::new(last, y)],
                                beam_stroke,
                            );
                        } else if run_start + 1 < flags.len() {
                            painter.line_segment(
                                [Pos2::new(first, y), Pos2::new(first + 5.0, y)],
                                beam_stroke,
                            );
                        } else {
                            painter.line_segment(
                                [Pos2::new(first - 5.0, y), Pos2::new(first, y)],
                                beam_stroke,
                            );
                        }
                    }
                }
                beamed.extend(flags.iter().map(|(position, _)| *position));
            }

            for (position, item) in score.items.iter().enumerate() {
                let ScoreItem::Chord(chord) = item else {
                    continue;
                };
                let x = stem_x(position);
                let selected = chord
                    .heads
                    .iter()
                    .any(|head| self.is_note_selected(head.note_id));
                let color = if selected {
                    self.theme.accent_color
                } else {
                    ink
                };
                if chord.value.has_stem() {
                    painter.line_segment(
                        [Pos2::new(x, top), Pos2::new(x, tip)],
                        Stroke::new(1.2_f32, color),
                    );
                    if !beamed.contains(&position) {
                        for flag in 0..chord.value.flags() {
                            let y = tip - flag as f32 * 3.5;
                            painter.line_segment(
                                [Pos2::new(x, y), Pos2::new(x + 4.5, y - 4.0)],
                                Stroke::new(1.5_f32, color),
                            );
                        }
                    }
                } else {
                    // A whole note has no stem: a small open head instead.
                    painter.add(egui::epaint::EllipseShape::stroke(
                        Pos2::new(x, top + 5.0),
                        Vec2::new(3.6, 2.6),
                        Stroke::new(1.2_f32, color),
                    ));
                }
                let dot_y = if chord.value.has_stem() {
                    (top + tip) * 0.5
                } else {
                    top + 5.0
                };
                for dot in 0..chord.value.dots {
                    painter.circle_filled(
                        Pos2::new(x + 5.0 + dot as f32 * 3.5, dot_y),
                        1.3,
                        color,
                    );
                }
            }
        }
    }

    fn paint_tab_number(
        &self,
        painter: &egui::Painter,
        track: &InstrumentTrack,
        left: f32,
        tab_y: f32,
        system: &StaffSystem,
        note: &crate::core::project::TabNote,
    ) {
        let x = self.content_x(system, left, note.abs_tick);
        let y = tab_y + note.string_index as f32 * SystemMetrics::STRING_GAP;
        let selected = self.is_note_selected(note.id);
        let predecessor = track.tie_predecessor(note);

        let text_color = if selected {
            self.theme.accent_color
        } else {
            self.theme.notation_foreground
        };
        let label = if predecessor.is_some() {
            format!("({})", note.fret)
        } else {
            note.fret.to_string()
        };
        let galley = painter.layout_no_wrap(label, FontId::monospace(11.0), text_color);
        let rect = Rect::from_center_size(Pos2::new(x, y), galley.size());
        // The backing patch keeps the string line from striking through the
        // number.
        let backing = rect.expand2(Vec2::new(1.5, 0.0));
        painter.rect_filled(backing, 1.0, self.theme.sheet_background);
        if selected {
            let [r, g, b, _] = self.theme.accent_color.to_array();
            painter.rect_filled(
                backing,
                2.0,
                egui::Color32::from_rgba_unmultiplied(r, g, b, 50),
            );
        }
        painter.galley(rect.min, galley, text_color);
        if note.effects.palm_mute && predecessor.is_none() {
            painter.text(
                Pos2::new(
                    x - 6.0,
                    tab_y + self.selected_string_span() + self.palm_mute_label_gap(),
                ),
                Align2::LEFT_TOP,
                "P.M.",
                FontId::monospace(8.0),
                self.theme.muted_foreground,
            );
        }

        let Some(previous) = predecessor else {
            return;
        };
        let previous_in_row = previous.abs_tick >= system.start_tick;
        let previous_x = self.content_x(system, left, previous.abs_tick);
        let tie_end = backing.left() - 1.0;
        let tie_start = if previous_in_row {
            previous_x + 8.0
        } else {
            left - 2.0
        }
        .min(tie_end - 6.0);
        paint_tie_arc(
            painter,
            Pos2::new(tie_start, y - 6.0),
            Pos2::new(tie_end, y - 6.0),
            self.theme.notation_foreground,
            true,
        );
    }

    /// The on-screen rectangle of the part of `note` that falls in one row.
    fn brick_fragment_rect(
        &self,
        left: f32,
        tab_y: f32,
        system_origin_tick: Tick,
        system_right: f32,
        note: &crate::core::project::TabNote,
        fragment_start: Tick,
        fragment_end: Tick,
        is_onset: bool,
    ) -> Rect {
        let x = left + (fragment_start - system_origin_tick) as f32 * self.pixels_per_tick;
        let tab_line_y = tab_y + note.string_index as f32 * 9.0;

        // Fragment width is strictly proportional to however much of the
        // note's duration falls in this row. The onset fragment is floored
        // wide enough to keep the fret digit legible (two-digit frets need a
        // touch more room); continuation fragments carry no digit, so no
        // floor is needed. Either way it's capped at the row's right wall -
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
        Rect::from_center_size(
            Pos2::new(x + width * 0.5, tab_line_y),
            Vec2::new(width, height),
        )
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
        let selected = self.is_note_selected(note.id);
        let note_rect = self.brick_fragment_rect(
            left,
            tab_y,
            system_origin_tick,
            system_right,
            note,
            fragment_start,
            fragment_end,
            is_onset,
        );
        let width = note_rect.width();

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

        // A user-tied brick stays a separate block; an arc over the seam
        // shows it sounds as a continuation of the brick before it.
        if is_onset
            && self
                .project
                .track(self.selected_track_id)
                .is_some_and(|track| track.is_tied_continuation(note))
        {
            paint_tie_arc(
                painter,
                Pos2::new(x - 8.0, tab_line_y - 8.0),
                Pos2::new(x + 8.0, tab_line_y - 8.0),
                self.theme.notation_foreground,
                true,
            );
        }

        // A brick whose length is exactly a dotted value shows its dots at
        // its end, if there is room beside the fret number.
        if fragment_end == note_end
            && width >= 20.0
            && let Some((_, dots)) = crate::core::length::detect_length(note.duration_ticks)
        {
            for dot in 0..dots {
                painter.circle_filled(
                    Pos2::new(note_rect.right() - 3.5 - dot as f32 * 3.5, note_rect.center().y),
                    1.2,
                    if selected {
                        egui::Color32::WHITE
                    } else {
                        self.theme.notation_foreground
                    },
                );
            }
        }

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
                    Pos2::new(x + 2.0, tab_y + self.selected_string_span() + 13.0),
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
        system: &StaffSystem,
    ) {
        let system_origin_tick = system.start_tick;
        // Only render if the cursor is inside this system's tick window.
        if self.cursor.tick < system_origin_tick
            || self.cursor.tick >= system_origin_tick + system.tick_span()
        {
            return;
        }
        let string_gap = 9.0;
        // Brick view mirrors the brick that will land there: the cursor box
        // is as wide as the selected duration, with a small floor so it
        // stays visible at the shortest durations. Tab view marks the beat
        // with a compact box around the fret position instead.
        let cell_width = if system.has_beat_layout() {
            TAB_CURSOR_WIDTH
        } else {
            (self.entry_ticks() as f32 * self.pixels_per_tick).max(7.0)
        };
        let x = if system.has_beat_layout() {
            self.content_x(system, left, self.cursor.tick) - cell_width * 0.5
        } else {
            left + system.x_of(self.cursor.tick, self.pixels_per_tick)
        };
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

    /// Finds the note (if any) whose visible fragment - onset or
    /// continuation, in whichever row `pos` falls in - contains `pos`.
    /// Shared by click-to-select and drag-start (which uses it to decide
    /// whether a drag begins a rubber-band area-selection or just grabs an
    /// existing note).
    fn find_note_fragment_at(&self, pos: Pos2, page: Rect) -> Option<u64> {
        if !page.contains(pos) {
            return None;
        }
        let track = self.project.track(self.selected_track_id)?;
        let PageGeometry {
            left,
            first_top,
            systems,
            metrics,
            ..
        } = self.page_geometry(track, page);
        let system_index = (((pos.y - first_top) / metrics.height).floor().max(0.0) as usize)
            .min(systems.len().saturating_sub(1));
        let system = systems.get(system_index)?;
        let tab_y = first_top + system_index as f32 * metrics.height + metrics.tab_offset;

        // Both a note's onset and a brick's continuation into this row (see
        // `paint_note_fragment`) are hits. If continuations weren't, a click
        // on the carried-over part would miss the note and a new one could
        // be typed on top of it.
        track.notes.iter().find_map(|note| {
            let rect = self.note_hit_rect(note, system, left, tab_y)?;
            rect.contains(pos).then_some(note.id)
        })
    }

    /// The clickable area of `note`'s visible piece in the system spanning
    /// `[system_origin_tick, system_end_tick)`, or `None` if it has no piece
    /// there. In brick view that's the brick's fragment; in tab view it's
    /// the small box around the fret number at the note's onset.
    fn note_hit_rect(
        &self,
        note: &crate::core::project::TabNote,
        system: &StaffSystem,
        left: f32,
        tab_y: f32,
    ) -> Option<Rect> {
        let system_origin_tick = system.start_tick;
        let system_end_tick = system_origin_tick + system.tick_span();
        let note_end = note.end_tick();
        if note_end <= system_origin_tick || note.abs_tick >= system_end_tick {
            return None;
        }
        let y = tab_y + note.string_index as f32 * SystemMetrics::STRING_GAP;
        match self.view_mode {
            EditorViewMode::Bricks => {
                let fragment_start = note.abs_tick.max(system_origin_tick);
                let fragment_end = note_end.min(system_end_tick);
                let x = left + system.x_of(fragment_start, self.pixels_per_tick);
                let width =
                    ((fragment_end - fragment_start) as f32 * self.pixels_per_tick).max(22.0);
                Some(Rect::from_center_size(
                    Pos2::new(x + width * 0.5, y),
                    Vec2::new(width, 14.0),
                ))
            }
            EditorViewMode::TabNumbers => {
                if note.abs_tick < system_origin_tick {
                    return None;
                }
                let x = self.content_x(system, left, note.abs_tick);
                Some(Rect::from_center_size(
                    Pos2::new(x, y),
                    Vec2::new(18.0, 14.0),
                ))
            }
        }
    }

    /// Every note on the selected track with a fragment (onset or
    /// continuation, in any visible row) intersecting `selection_rect`.
    /// Used by the canvas's rubber-band area-selection.
    fn notes_intersecting_rect(&self, selection_rect: Rect, page: Rect) -> Vec<u64> {
        let Some(track) = self.project.track(self.selected_track_id) else {
            return Vec::new();
        };
        let PageGeometry {
            left,
            first_top,
            systems,
            metrics,
            ..
        } = self.page_geometry(track, page);

        let mut hits = Vec::new();
        for (system_index, system) in systems.iter().enumerate() {
            let top = first_top + system_index as f32 * metrics.height;
            if top > page.bottom() {
                break;
            }
            if top + metrics.height < page.top() {
                continue;
            }
            let tab_y = top + metrics.tab_offset;

            for note in &track.notes {
                let Some(note_rect) = self.note_hit_rect(note, system, left, tab_y) else {
                    continue;
                };
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
            self.status = format!(
                "Selected note {note_id}: {}.",
                length_description(note.duration_ticks)
            );
            return;
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
    /// count as a grabbable edge - not the open side of a continuation
    /// fragment where it merely wraps into a new row (see
    /// `paint_note_fragment`); there's no real boundary there to drag.
    ///
    /// Checks every note in the current selection (single click-select or
    /// multi area-select, whichever is active) and returns the first one
    /// whose edge `pos` is near, along with which edge - grabbing *any*
    /// selected note's edge is enough to start stretching the whole
    /// selection together (see `begin_canvas_drag`).
    fn stretch_edge_at(&self, pos: Pos2, page: Rect) -> Option<(u64, StretchEdge)> {
        // Fret numbers have no edges to grab.
        if self.view_mode == EditorViewMode::TabNumbers {
            return None;
        }
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

        let PageGeometry {
            left,
            first_top,
            systems,
            metrics,
            ..
        } = self.page_geometry(track, page);
        let system_height = metrics.height;
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
        // not the project's tick 0 - dragging within a bar should feel
        // anchored to that bar's own grid.
        let (bar_start, _) = track.measure_bounds_at(anchor_original_tick);
        let grid = self.drag_step_ticks.max(1) as f64;
        let offset_from_bar_start = anchor_raw_tick - bar_start as f64;
        let nearest_grid_tick = bar_start as f64 + (offset_from_bar_start / grid).round() * grid;
        // Elastic snap: only pull to the grid line when close to it, and
        // recomputed fresh each call from the fixed original position - a
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
            if already_selected && self.view_mode == EditorViewMode::TabNumbers {
                // Tied notes in tab view can't be dragged around without
                // breaking their chains, so a drag on a selected number
                // leaves the selection alone.
                self.canvas_drag = CanvasDragMode::Idle;
            } else if already_selected {
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

    /// The edit the in-progress move or stretch drag would make, or `None`
    /// when no note drag is under way.
    fn note_drag_plan(&self, snapping_disabled: bool) -> Option<NoteDragPlan> {
        match &self.canvas_drag {
            CanvasDragMode::Stretching {
                note_ids,
                edge,
                start_pos,
                current_pos,
            } => Some(NoteDragPlan {
                edit: NoteDragEdit::Stretch {
                    edge: *edge,
                    delta_ticks: ((current_pos.x - start_pos.x) / self.pixels_per_tick).round()
                        as Tick,
                },
                note_ids: note_ids.clone(),
                pointer: *current_pos,
                snapping_disabled,
            }),
            CanvasDragMode::MovingNotes {
                note_ids,
                anchor_note_id,
                start_pos,
                current_pos,
            } => {
                let raw_delta_ticks =
                    (current_pos.x - start_pos.x) as f64 / self.pixels_per_tick as f64;
                // Each string is already a discrete unit, so vertical
                // movement just rounds to the nearest one - no snap grid
                // or threshold needed the way the tick axis has.
                let string_delta =
                    ((current_pos.y - start_pos.y) / SystemMetrics::STRING_GAP).round() as i32;
                Some(NoteDragPlan {
                    edit: NoteDragEdit::Move {
                        offset_ticks: self.compute_move_snap_offset(
                            *anchor_note_id,
                            raw_delta_ticks,
                            snapping_disabled,
                        ),
                        string_delta,
                    },
                    note_ids: note_ids.clone(),
                    pointer: *current_pos,
                    snapping_disabled,
                })
            }
            _ => None,
        }
    }

    fn apply_note_drag_plan(&mut self, plan: &NoteDragPlan) {
        let track_id = self.selected_track_id;
        let previous_project = self.project.clone();
        let changed = apply_note_drag(
            &mut self.project,
            track_id,
            plan,
            self.stretch_step_ticks,
        );
        if changed == 0 {
            return;
        }
        self.record_project_edit(previous_project);
        self.refresh_after_edit(track_id);
        let several = plan.note_ids.len() > 1;
        self.status = match plan.edit {
            NoteDragEdit::Stretch { edge, .. } => {
                let edge_label = match edge {
                    StretchEdge::Left => "left",
                    StretchEdge::Right => "right",
                };
                if several {
                    format!("Stretched the {edge_label} edge of {changed} notes.")
                } else {
                    format!("Stretched the {edge_label} edge of note {}.", plan.note_ids[0])
                }
            }
            NoteDragEdit::Move { .. } => {
                if several {
                    format!("Moved {changed} notes.")
                } else {
                    format!("Moved note {}.", plan.note_ids[0])
                }
            }
        };
    }

    fn finish_canvas_drag(&mut self, page: Rect, snapping_disabled: bool) {
        if let Some(plan) = self.note_drag_plan(snapping_disabled) {
            self.apply_note_drag_plan(&plan);
        }
        if let CanvasDragMode::AreaSelecting {
            start_pos,
            current_pos,
        } = &self.canvas_drag
        {
            let selection_rect = Rect::from_two_pos(*start_pos, *current_pos);
            self.area_selected_note_ids = self.notes_intersecting_rect(selection_rect, page);
            self.selected_note_id = None;
            if !self.area_selected_note_ids.is_empty() {
                self.status = format!("{} notes selected.", self.area_selected_note_ids.len());
            }
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
        let PageGeometry {
            left,
            first_top,
            systems,
            metrics,
            ..
        } = self.page_geometry(track, page);
        let system_height = metrics.height;
        let system_index = (((pos.y - first_top) / system_height).floor().max(0.0) as usize)
            .min(systems.len().saturating_sub(1));
        let Some(system) = systems.get(system_index) else {
            return;
        };
        let system_origin_tick = system.start_tick;
        // Tab view snaps to the nearest beat; brick view to a sixteenth.
        let snapped_tick = if system.has_beat_layout() {
            system.tick_at(pos.x - left, self.pixels_per_tick)
        } else {
            let clicked_tick = ((((pos.x - left) / self.pixels_per_tick).max(0.0)) as Tick
                + system_origin_tick)
                .min(system.end_tick.max(system_origin_tick));
            snap_tick(clicked_tick, TICKS_PER_QUARTER / 4)
        };
        let tab_y =
            first_top + system_index as f32 * system_height + metrics.tab_offset;
        let string_gap = 9.0;

        let string_index = ((pos.y - tab_y + string_gap * 0.5) / string_gap)
            .floor()
            .clamp(0.0, track.tuning.len().saturating_sub(1) as f32)
            as usize;

        // A click on empty space moves the keyboard cursor there so the user
        // can click, then type the fret (digits + Enter). Notes are only ever
        // inserted from the keyboard.
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

/// Where playback begins: the first selected note, else the edit cursor.
/// Selecting a note moves the cursor onto it, but a selection that was then
/// dragged elsewhere still wins over the stale cursor.
fn playback_start(
    track: Option<&InstrumentTrack>,
    selected_note_id: Option<u64>,
    area_selected_note_ids: &[u64],
    cursor_tick: Tick,
) -> Tick {
    let selected_start = track.and_then(|track| {
        track
            .notes
            .iter()
            .filter(|note| {
                selected_note_id == Some(note.id) || area_selected_note_ids.contains(&note.id)
            })
            .map(|note| note.abs_tick)
            .min()
    });
    selected_start.unwrap_or(cursor_tick).max(0)
}

/// Track table columns: heading and width. Fixed widths let the header and
/// the "All sounds" checkbox sit outside the scrolling list yet line up
/// with its cells.
const TRACK_COLUMNS: [(&str, f32); 8] = [
    ("N", 28.0),
    ("S", 24.0),
    ("M", 24.0),
    ("Name", 130.0),
    ("Sound", 180.0),
    ("Vol", 52.0),
    ("Pan", 52.0),
    ("Ch", 28.0),
];
const SOUND_COLUMN: usize = 4;
const TRACK_COLUMN_GAP: f32 = 4.0;
const TRACK_ROW_HEIGHT: f32 = 22.0;

/// One fixed-width, vertically centred cell of the track table.
fn table_cell<R>(ui: &mut egui::Ui, width: f32, add_contents: impl FnOnce(&mut egui::Ui) -> R) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, TRACK_ROW_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            // An allocated child shrinks to what it holds unless told otherwise.
            ui.set_min_size(Vec2::new(width, TRACK_ROW_HEIGHT));
            add_contents(ui);
        },
    );
}

fn note_count(count: usize) -> String {
    if count == 1 {
        "1 note".to_owned()
    } else {
        format!("{count} notes")
    }
}

/// What a brick's length is, as a player would say it: a note value with its
/// dots when the length is exactly one, otherwise the raw tick count.
fn length_description(ticks: Tick) -> String {
    match DurationChoice::detect(ticks) {
        Some((choice, dots)) => {
            let dotting = match dots {
                0 => "",
                1 => "dotted ",
                _ => "double-dotted ",
            };
            format!(
                "{dotting}{} note ({})",
                duration_name(choice).to_lowercase(),
                choice.label_dotted(dots)
            )
        }
        None => format!("{ticks} ticks"),
    }
}

/// Turns a drag value's response into the events the track table applies:
/// edits while it changes, and the end of the undo step once a drag stops or
/// a typed value is accepted.
fn push_mix_event(
    events: &mut Vec<MixEvent>,
    track_id: u64,
    response: &egui::Response,
    edit: MixEdit,
) {
    let changed = response.changed();
    let commit = response.drag_stopped() || (changed && !response.dragged());
    if changed || commit {
        events.push(MixEvent {
            track_id,
            edit: changed.then_some(edit),
            commit,
        });
    }
}

fn pan_label(pan: i8) -> String {
    match pan {
        0 => "C".to_owned(),
        pan if pan < 0 => format!("L{}", -(pan as i16)),
        pan => format!("R{pan}"),
    }
}

/// Accepts `C`, `L20`, `R5` or a plain number (negative is left).
fn parse_pan(text: &str) -> Option<f64> {
    let text = text.trim();
    let magnitude = |rest: &str| rest.trim().parse::<f64>().ok();
    match text.chars().next()?.to_ascii_uppercase() {
        'C' => Some(0.0),
        'L' => magnitude(&text[1..]).map(|value| -value),
        'R' => magnitude(&text[1..]),
        _ => text.parse().ok(),
    }
}

/// Width of the edit cursor box in tab view.
const TAB_CURSOR_WIDTH: f32 = 18.0;

/// Horizontal offset from a note's onset to the center of its fret number in
/// tab view, so a number at tick 0 clears the barline.
const TAB_NUMBER_CENTER_OFFSET: f32 = 7.0;

/// Draws a tie as a shallow arc between `from` and `to`, bulging upward when
/// `up` is set and downward otherwise.
fn paint_tie_arc(painter: &egui::Painter, from: Pos2, to: Pos2, color: egui::Color32, up: bool) {
    let bulge = if up { -9.0 } else { 9.0 };
    let control = Pos2::new(
        (from.x + to.x) * 0.5,
        if up {
            from.y.min(to.y) + bulge
        } else {
            from.y.max(to.y) + bulge
        },
    );
    painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
        [from, control, to],
        false,
        egui::Color32::TRANSPARENT,
        Stroke::new(1.2_f32, color),
    ));
}

/// Whether the bar starting at `bar_start` gets a time signature label: the
/// first bar of the piece and every bar where the signature changes.
fn signature_label_at(track: &InstrumentTrack, bar_start: Tick) -> bool {
    bar_start == 0 || track.signature_at(bar_start - 1) != track.signature_at(bar_start)
}

/// Draws a rest glyph centered on `cx`. `y` is the staff line the glyph
/// attaches to: a whole rest hangs below it, a half rest sits on it, and
/// shorter rests are centered on it.
fn paint_rest_symbol(
    painter: &egui::Painter,
    cx: f32,
    y: f32,
    value: notation::NoteValue,
    color: egui::Color32,
) {
    let stroke = Stroke::new(1.8_f32, color);
    let line = Stroke::new(1.0_f32, color);
    match value.value {
        1 => {
            painter.line_segment([Pos2::new(cx - 7.0, y), Pos2::new(cx + 7.0, y)], line);
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(cx - 5.0, y), Pos2::new(cx + 5.0, y + 3.8)),
                0.0,
                color,
            );
        }
        2 => {
            painter.line_segment([Pos2::new(cx - 7.0, y), Pos2::new(cx + 7.0, y)], line);
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(cx - 5.0, y - 3.8), Pos2::new(cx + 5.0, y)),
                0.0,
                color,
            );
        }
        4 => {
            painter.add(egui::Shape::line(
                vec![
                    Pos2::new(cx - 2.0, y - 9.0),
                    Pos2::new(cx + 3.0, y - 4.0),
                    Pos2::new(cx - 3.0, y + 1.0),
                    Pos2::new(cx + 3.0, y + 6.0),
                ],
                stroke,
            ));
        }
        _ => {
            // Eighth and shorter: a slash with one dot per flag.
            painter.line_segment(
                [Pos2::new(cx + 3.0, y - 8.0), Pos2::new(cx - 3.0, y + 8.0)],
                stroke,
            );
            for flag in 0..value.flags().clamp(1, 5) {
                let t = flag as f32 * 0.25;
                painter.circle_filled(
                    Pos2::new(cx - 6.0 * t - 2.5, y - 8.0 + 16.0 * t),
                    1.8,
                    color,
                );
            }
        }
    }
    for dot in 0..value.dots {
        painter.circle_filled(Pos2::new(cx + 8.0 + dot as f32 * 4.0, y - 2.0), 1.3, color);
    }
}

/// Draws a sharp or a natural sign centered on `(x, y)`.
fn paint_accidental(painter: &egui::Painter, x: f32, y: f32, sharp: bool, color: egui::Color32) {
    let stroke = Stroke::new(1.0_f32, color);
    let thick = Stroke::new(1.8_f32, color);
    painter.line_segment(
        [Pos2::new(x - 1.7, y - 6.0), Pos2::new(x - 1.7, y + 5.0)],
        stroke,
    );
    painter.line_segment(
        [Pos2::new(x + 1.7, y - 5.0), Pos2::new(x + 1.7, y + 6.0)],
        stroke,
    );
    if sharp {
        painter.line_segment(
            [Pos2::new(x - 3.6, y - 1.0), Pos2::new(x + 3.6, y - 3.0)],
            thick,
        );
        painter.line_segment(
            [Pos2::new(x - 3.6, y + 3.0), Pos2::new(x + 3.6, y + 1.0)],
            thick,
        );
    } else {
        painter.line_segment(
            [Pos2::new(x - 1.7, y - 2.0), Pos2::new(x + 1.7, y - 3.0)],
            thick,
        );
        painter.line_segment(
            [Pos2::new(x - 1.7, y + 3.0), Pos2::new(x + 1.7, y + 2.0)],
            thick,
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
/// ranks an in-order match above one that merely overlaps by character.
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

/// A submenu button that - unlike plain `ui.menu_button` - closes only when
/// clicking outside it, not when clicking a widget *inside* it. egui's menus
/// default to `PopupCloseBehavior::CloseOnClick`, which closes on *any*
/// click, so an interactive control nested in a submenu (the Stretching
/// submenu's DragValue) needs this to stay open while it's being used.
/// Actions that should close the menu do so with their own `ui.close()`.
/// Inside a menu it must be a `SubMenuButton`: that is the only button that
/// opens on hover and nests under its parent. A `MenuButton` is a standalone
/// click-to-toggle popup, which never expands and swallows the click.
fn submenu<R>(ui: &mut egui::Ui, label: &str, add_contents: impl FnOnce(&mut egui::Ui) -> R) {
    use egui::containers::menu::{MenuButton, MenuConfig, SubMenuButton, is_in_menu};
    let config = MenuConfig::default().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    if is_in_menu(ui) {
        SubMenuButton::new(label).config(config).ui(ui, add_contents);
    } else {
        MenuButton::new(label).config(config).ui(ui, add_contents);
    }
}

/// A `DragValue` that scrolls through a fixed list of values (by index)
/// instead of a raw numeric range - the same drag-to-change / click-and-
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

const PALETTE_BUTTON: f32 = 22.0;
const PALETTE_GAP: f32 = 2.0;
const PALETTE_COLUMNS: usize = 8;
/// Space left of the first and right of the last button in a palette row.
const PALETTE_SIDE_PAD: f32 = 9.0;
const PALETTE_WIDTH: f32 = PALETTE_COLUMNS as f32 * PALETTE_BUTTON
    + (PALETTE_COLUMNS - 1) as f32 * PALETTE_GAP
    + 2.0 * PALETTE_SIDE_PAD;

/// Dynamics and the MIDI velocity each one writes.
const DYNAMICS: [(&str, u8); 8] = [
    ("ppp", 30),
    ("pp", 42),
    ("p", 56),
    ("mp", 72),
    ("mf", 88),
    ("f", 104),
    ("ff", 116),
    ("fff", 127),
];

/// Note effects the palette toggles; each maps to one `NoteEffects` flag.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EffectKind {
    Ghost,
    Dead,
    Accent,
    HeavyAccent,
    Staccato,
    LetRing,
    PalmMute,
    Vibrato,
    Slide,
    HammerOn,
    Tapping,
    Slap,
    Pop,
    FadeIn,
}

impl EffectKind {
    const ALL: [EffectKind; 14] = [
        EffectKind::Ghost,
        EffectKind::Dead,
        EffectKind::Accent,
        EffectKind::HeavyAccent,
        EffectKind::Staccato,
        EffectKind::LetRing,
        EffectKind::PalmMute,
        EffectKind::Vibrato,
        EffectKind::Slide,
        EffectKind::HammerOn,
        EffectKind::Tapping,
        EffectKind::Slap,
        EffectKind::Pop,
        EffectKind::FadeIn,
    ];

    fn flag(self, effects: &mut NoteEffects) -> &mut bool {
        match self {
            EffectKind::Ghost => &mut effects.ghost,
            EffectKind::Dead => &mut effects.dead,
            EffectKind::Accent => &mut effects.accent,
            EffectKind::HeavyAccent => &mut effects.heavy_accent,
            EffectKind::Staccato => &mut effects.staccato,
            EffectKind::LetRing => &mut effects.let_ring,
            EffectKind::PalmMute => &mut effects.palm_mute,
            EffectKind::Vibrato => &mut effects.vibrato,
            EffectKind::Slide => &mut effects.slide,
            EffectKind::HammerOn => &mut effects.hammer,
            EffectKind::Tapping => &mut effects.tapping,
            EffectKind::Slap => &mut effects.slapping,
            EffectKind::Pop => &mut effects.popping,
            EffectKind::FadeIn => &mut effects.fade_in,
        }
    }

    fn is_set(self, effects: &NoteEffects) -> bool {
        match self {
            EffectKind::Ghost => effects.ghost,
            EffectKind::Dead => effects.dead,
            EffectKind::Accent => effects.accent,
            EffectKind::HeavyAccent => effects.heavy_accent,
            EffectKind::Staccato => effects.staccato,
            EffectKind::LetRing => effects.let_ring,
            EffectKind::PalmMute => effects.palm_mute,
            EffectKind::Vibrato => effects.vibrato,
            EffectKind::Slide => effects.slide,
            EffectKind::HammerOn => effects.hammer,
            EffectKind::Tapping => effects.tapping,
            EffectKind::Slap => effects.slapping,
            EffectKind::Pop => effects.popping,
            EffectKind::FadeIn => effects.fade_in,
        }
    }

    fn icon(self) -> PaletteIcon {
        match self {
            EffectKind::Ghost => PaletteIcon::Ghost,
            EffectKind::Dead => PaletteIcon::Dead,
            EffectKind::Accent => PaletteIcon::Accent,
            EffectKind::HeavyAccent => PaletteIcon::HeavyAccent,
            EffectKind::Staccato => PaletteIcon::Staccato,
            EffectKind::LetRing => PaletteIcon::LetRing,
            EffectKind::PalmMute => PaletteIcon::PalmMute,
            EffectKind::Vibrato => PaletteIcon::Vibrato,
            EffectKind::Slide => PaletteIcon::Slide,
            EffectKind::HammerOn => PaletteIcon::HammerOn,
            EffectKind::Tapping => PaletteIcon::Tapping,
            EffectKind::Slap => PaletteIcon::Slap,
            EffectKind::Pop => PaletteIcon::Pop,
            EffectKind::FadeIn => PaletteIcon::FadeIn,
        }
    }

    fn tooltip(self) -> &'static str {
        match self {
            EffectKind::Ghost => "Ghost note",
            EffectKind::Dead => "Dead note",
            EffectKind::Accent => "Accentuated note",
            EffectKind::HeavyAccent => "Heavily accentuated note",
            EffectKind::Staccato => "Staccato",
            EffectKind::LetRing => "Let ring",
            EffectKind::PalmMute => "Palm mute",
            EffectKind::Vibrato => "Vibrato",
            EffectKind::Slide => "Slide",
            EffectKind::HammerOn => "Hammer-on / pull-off",
            EffectKind::Tapping => "Tapping",
            EffectKind::Slap => "Slapping",
            EffectKind::Pop => "Popping",
            EffectKind::FadeIn => "Fade in",
        }
    }
}

fn duration_icon(duration: DurationChoice) -> PaletteIcon {
    match duration {
        DurationChoice::Whole => PaletteIcon::Whole,
        DurationChoice::Half => PaletteIcon::Half,
        DurationChoice::Quarter => PaletteIcon::Quarter,
        DurationChoice::Eighth => PaletteIcon::Eighth,
        DurationChoice::Sixteenth => PaletteIcon::Sixteenth,
        DurationChoice::ThirtySecond => PaletteIcon::ThirtySecond,
    }
}

fn duration_name(duration: DurationChoice) -> &'static str {
    match duration {
        DurationChoice::Whole => "Whole",
        DurationChoice::Half => "Half",
        DurationChoice::Quarter => "Quarter",
        DurationChoice::Eighth => "Eighth",
        DurationChoice::Sixteenth => "Sixteenth",
        DurationChoice::ThirtySecond => "Thirty-second",
    }
}

/// One icon button of the edition palette. `active` marks the current
/// duration, dynamic or effect; a disabled button stands for a command that
/// is on the roadmap but not implemented yet.
fn palette_button(
    ui: &mut egui::Ui,
    theme: &EditorTheme,
    icon: PaletteIcon,
    active: bool,
    enabled: bool,
    tooltip: &str,
) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(PALETTE_BUTTON), sense);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if active {
            painter.rect_filled(rect, 3.0, theme.accent_color.gamma_multiply(0.28));
            painter.rect_stroke(
                rect,
                3.0,
                Stroke::new(1.0_f32, theme.accent_color),
                StrokeKind::Inside,
            );
        } else if enabled && response.hovered() {
            painter.rect_filled(rect, 3.0, theme.palette_hover);
        }
        let color = if enabled {
            theme.palette_icon
        } else {
            theme.palette_icon.gamma_multiply(0.32)
        };
        icons::paint(painter, rect, icon, color);
    }
    if enabled {
        response.on_hover_text(tooltip)
    } else {
        response.on_hover_text(format!("{tooltip} (planned)"))
    }
}

fn planned_palette_button(
    ui: &mut egui::Ui,
    theme: &EditorTheme,
    icon: PaletteIcon,
    tooltip: &str,
) {
    palette_button(ui, theme, icon, false, false, tooltip);
}

fn palette_row(ui: &mut egui::Ui, add_buttons: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(PALETTE_SIDE_PAD - ui.spacing().item_spacing.x);
        add_buttons(ui);
    });
}

fn palette_separator(ui: &mut egui::Ui, theme: &EditorTheme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(PALETTE_WIDTH, 9.0), Sense::hover());
    let y = rect.center().y;
    ui.painter().line_segment(
        [
            Pos2::new(rect.left() + PALETTE_SIDE_PAD, y),
            Pos2::new(rect.right() - PALETTE_SIDE_PAD, y),
        ],
        Stroke::new(1.0_f32, theme.palette_separator),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_bar_beat_system() -> StaffSystem {
        let mut system = StaffSystem::from_bars(vec![(0, 3840), (3840, 7680)]);
        system.beat_bars = vec![
            BeatBarLayout {
                start: 0,
                end: 3840,
                x: 0.0,
                width: 100.0,
                columns: vec![(0, 10.0), (960, 40.0), (1920, 70.0)],
            },
            BeatBarLayout {
                start: 3840,
                end: 7680,
                x: 100.0,
                width: 100.0,
                columns: vec![(3840, 110.0), (5760, 160.0)],
            },
        ];
        system
    }

    fn note_snapshot(project: &BwxProject, id: u64) -> (Tick, usize, Tick) {
        let note = project
            .track(1)
            .unwrap()
            .notes
            .iter()
            .find(|note| note.id == id)
            .unwrap();
        (note.abs_tick, note.string_index, note.duration_ticks)
    }

    fn stretch_plan(delta_ticks: Tick) -> NoteDragPlan {
        NoteDragPlan {
            edit: NoteDragEdit::Stretch {
                edge: StretchEdge::Right,
                delta_ticks,
            },
            note_ids: vec![1],
            pointer: Pos2::ZERO,
            snapping_disabled: false,
        }
    }

    #[test]
    fn a_drag_plan_changes_only_the_notes_it_names() {
        let mut project = BwxProject::demo();
        let untouched = note_snapshot(&project, 2);
        let changed = apply_note_drag(&mut project, 1, &stretch_plan(960), 240);
        assert_eq!(changed, 1);
        assert_eq!(note_snapshot(&project, 2), untouched);
        assert_eq!(note_snapshot(&project, 1).2, 960 + 960);
    }

    #[test]
    fn a_scratch_copy_previews_exactly_what_the_real_project_gets() {
        let plan = NoteDragPlan {
            edit: NoteDragEdit::Move {
                offset_ticks: 1920,
                string_delta: -2,
            },
            note_ids: vec![1],
            pointer: Pos2::ZERO,
            snapping_disabled: false,
        };
        let mut real = BwxProject::demo();
        let mut scratch = real.clone();
        let original = note_snapshot(&real, 1);
        apply_note_drag(&mut scratch, 1, &plan, 240);
        assert_eq!(note_snapshot(&real, 1), original, "preview must not edit the project");
        apply_note_drag(&mut real, 1, &plan, 240);
        assert_eq!(note_snapshot(&real, 1), note_snapshot(&scratch, 1));
        assert_ne!(note_snapshot(&real, 1), original);
    }

    #[test]
    fn a_zero_drag_changes_nothing() {
        let mut project = BwxProject::demo();
        let plan = NoteDragPlan {
            edit: NoteDragEdit::Move {
                offset_ticks: 0,
                string_delta: 0,
            },
            note_ids: vec![1],
            pointer: Pos2::ZERO,
            snapping_disabled: false,
        };
        assert_eq!(apply_note_drag(&mut project, 1, &plan, 240), 0);
    }

    #[test]
    fn playback_starts_at_the_cursor_without_a_selection() {
        let project = BwxProject::demo();
        let track = project.track(1);
        assert_eq!(playback_start(track, None, &[], 1920), 1920);
        assert_eq!(playback_start(track, None, &[], -5), 0);
        assert_eq!(playback_start(None, None, &[], 960), 960);
    }

    #[test]
    fn playback_starts_at_the_earliest_selected_note() {
        let project = BwxProject::demo();
        let track = project.track(1).unwrap();
        let start_of = |id: u64| {
            track
                .notes
                .iter()
                .find(|note| note.id == id)
                .unwrap()
                .abs_tick
        };
        assert_eq!(playback_start(Some(track), Some(2), &[], 0), start_of(2));
        let last = track.notes.iter().map(|note| note.id).max().unwrap();
        let earliest = start_of(1).min(start_of(last));
        assert_eq!(
            playback_start(Some(track), None, &[last, 1], 99_999),
            earliest
        );
    }

    #[test]
    fn note_counts_are_pluralised() {
        assert_eq!(note_count(1), "1 note");
        assert_eq!(note_count(4), "4 notes");
    }

    #[test]
    fn lengths_are_described_with_their_dots() {
        let q = TICKS_PER_QUARTER;
        assert_eq!(length_description(q), "quarter note (1/4)");
        assert_eq!(length_description(q * 3 / 2), "dotted quarter note (1/4.)");
        assert_eq!(
            length_description(q * 7 / 4),
            "double-dotted quarter note (1/4..)"
        );
        assert_eq!(length_description(q + 7), format!("{} ticks", q + 7));
    }

    #[test]
    fn pan_is_labelled_and_parsed_from_the_centre() {
        assert_eq!(pan_label(0), "C");
        assert_eq!(pan_label(-64), "L64");
        assert_eq!(pan_label(63), "R63");
        for pan in -64..=63_i8 {
            assert_eq!(parse_pan(&pan_label(pan)), Some(pan as f64));
        }
        assert_eq!(parse_pan("l12"), Some(-12.0));
        assert_eq!(parse_pan("-7"), Some(-7.0));
        assert_eq!(parse_pan("20"), Some(20.0));
        assert_eq!(parse_pan("nonsense"), None);
        assert_eq!(parse_pan(""), None);
    }

    fn three_row_geometry(first_top: f32) -> PageGeometry {
        PageGeometry {
            left: 54.0,
            right: 800.0,
            first_top,
            systems: vec![
                StaffSystem::from_bars(vec![(0, 3840), (3840, 7680)]),
                StaffSystem::from_bars(vec![(7680, 11520), (11520, 15360)]),
                StaffSystem::from_bars(vec![(15360, 19200)]),
            ],
            metrics: SystemMetrics::new(false, 6),
        }
    }

    #[test]
    fn page_rows_are_found_from_screen_height_and_clamped() {
        let geometry = three_row_geometry(100.0);
        let height = geometry.metrics.height;
        assert_eq!(geometry.system_index_at(0.0), 0);
        assert_eq!(geometry.system_index_at(100.0 + height * 1.5), 1);
        assert_eq!(geometry.system_index_at(100.0 + height * 99.0), 2);
    }

    #[test]
    fn scrolling_moves_every_row_by_the_same_amount() {
        let at_rest = three_row_geometry(100.0);
        let scrolled = three_row_geometry(60.0);
        for row in 0..3 {
            assert_eq!(at_rest.system_top(row) - scrolled.system_top(row), 40.0);
        }
    }

    #[test]
    fn rows_are_found_from_ticks() {
        let geometry = three_row_geometry(0.0);
        assert_eq!(geometry.system_index_of_tick(0), 0);
        assert_eq!(geometry.system_index_of_tick(7679), 0);
        assert_eq!(geometry.system_index_of_tick(7680), 1);
        assert_eq!(geometry.system_index_of_tick(19_000), 2);
        assert_eq!(geometry.system_index_of_tick(99_999), 2);
    }

    #[test]
    fn brick_layout_maps_ticks_linearly() {
        let system = StaffSystem::from_bars(vec![(960, 4800)]);
        assert_eq!(system.x_of(960, 0.1), 0.0);
        assert_eq!(system.x_of(1960, 0.1), 100.0);
        assert_eq!(system.bar_x(0, 0.1), 0.0);
        assert!((system.right_x(0.1) - 384.0).abs() < 0.01);
        assert_eq!(system.tick_at(100.0, 0.1), 1960);
    }

    #[test]
    fn beat_layout_puts_beats_at_their_columns() {
        let system = two_bar_beat_system();
        assert_eq!(system.x_of(0, 0.07), 10.0);
        assert_eq!(system.x_of(960, 0.07), 40.0);
        // The first beat of the second bar is in the second bar.
        assert_eq!(system.x_of(3840, 0.07), 110.0);
        assert_eq!(system.right_x(0.07), 200.0);
        assert_eq!(system.bar_x(1, 0.07), 100.0);
    }

    #[test]
    fn beat_layout_interpolates_between_beats_of_one_bar() {
        let system = two_bar_beat_system();
        assert_eq!(system.x_of(480, 0.07), 25.0);
        // After the last beat, toward the end of the bar.
        assert_eq!(system.x_of(2880, 0.07), 84.0);
    }

    #[test]
    fn clicking_in_beat_layout_picks_the_nearest_beat() {
        let system = two_bar_beat_system();
        assert_eq!(system.tick_at(45.0, 0.07), 960);
        assert_eq!(system.tick_at(5.0, 0.07), 0);
        assert_eq!(system.tick_at(150.0, 0.07), 5760);
        assert_eq!(system.tick_at(999.0, 0.07), 5760);
    }

    #[test]
    fn system_height_follows_the_notation_toggle_and_string_count() {
        let with_staff = SystemMetrics::new(true, 6);
        let without_staff = SystemMetrics::new(false, 6);
        assert_eq!(with_staff.tab_offset, 104.0);
        assert_eq!(with_staff.height, 104.0 + 45.0 + 41.0);
        assert_eq!(without_staff.height, 16.0 + 45.0 + 41.0);
        assert!(SystemMetrics::new(true, 8).height > with_staff.height);
    }
}
