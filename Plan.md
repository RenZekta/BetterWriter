# BetterWriter — Development Plan

Living checklist for the BetterWriter tablature editor (`BetterWriter`).
Built on the architecture described in `BetterWriterPrompt.md`: a pure-Rust
egui front end, a tick-based core music engine, a SoundFont/audio pipeline, and
a polymetric `.bwx` format.
Important: edit this file with its formatting in mind, don't change the formatting. Edit this plan each time anything is changed in the app. When a goal is fully finished, move it in implemented area.

---

## 19.06.2026 ✅ Implemented / Fixed this pass

### Keyboard-driven tab writing (Phase 1 — input layer)
- [x] Added an **edit cursor** (`EditCursor`: `tick` + `string_index` + pending
      multi-digit fret buffer) that marks where the next typed note lands.
- [x] **Fret entry from the keyboard**: typing `0`–`9` builds a fret number.
      Two quick digits compose (e.g. `1` then `2` → fret **12**); the buffer
      auto-expires after ~1.2 s of inactivity.
- [x] **Insert / overwrite at the cursor**: `Enter` or `Space` places the
      buffered fret (or the toolbar default) at the cursor; if a note already
      occupies that cell its fret is replaced instead of stacked.
- [x] **Delete**: `Backspace` / `Delete` clears a pending fret, otherwise
      removes the note under the cursor.
- [x] **Navigation**: `←`/`→` move the cursor by the selected note duration
      (TuxGuitar-style, bar-aware); `↑`/`↓` change the active string.
- [x] **Duration menu**: `-`/`+`/`=` cycle the active duration (`+`/`=` → longer note, no shift needed,
       `-`→ shorter note); `Ctrl+1`–`Ctrl+6` jump to a duration
       slot; toolbar `+` button and the combo box drive the same state.
- [x] `Esc` clears the buffer/selection; `Home` snaps the cursor to tick 0.
- [x] Mouse and keyboard workflows unified: clicking the staff now
      **repositions the cursor** (and selects the string) instead of silently
      inserting a fixed-fret note.

### Note brick sizing (Phase 1 — rendering)
- [x] Note bricks are now **strictly proportional** to their duration in ticks
      (a 1/32 is ~half the width of a 1/16 at default zoom), with a small
      digit-aware floor only for legibility, instead of all short notes being
      forced to the same 18 px width.
- [x] Font size shrinks and brick height reduces for very short notes so the
      visual rhythm matches the musical rhythm.
- [x] **Cursor cell** drawn on the tab staff: translucent accent box sized to
      the current duration, with the half-typed fret shown as ghost text.
- [x] Duration combo box wrapped in a horizontal `ScrollArea` so the toolbar
      never overflows when space is tight. (?????)
- [x] Removed redundant `−` button to the left of the duration dropdown.

### Toolbar / UX polish
- [x] `default fret` value renamed and given a tooltip clarifying it is only
      used when committing without typing a number first.

### Outline of remaining features (disabled-state hints)
- [x] Standard-format bundle exports (`.tg` / `.gp`) now respect the
      compatibility report: buttons are **grayed out** with the
      _"Polyrhythms detected, incompatible format."_ tooltip when the project
      is polymetric.

### Help / discoverability
- [x] **Help → Keyboard** menu lists the full keymap.

---

## 11.08.2026 ✅ Implemented / Fixed this pass

### Editing workflow
- [x] **Undo / redo stack** for score edits: a bounded 100-step project
      history covers note entry/replacement, deletion, duration, velocity,
      effects, and tempo. Available through **Edit → Undo/Redo**, `Ctrl+Z` /
      `Ctrl+Y`, and `Cmd+Z` / `Cmd+Shift+Z` on macOS.
- [x] RMB functionality partially implemented (check below for details).
- [x] **Per-track polymetric time signatures** in the editor UI: right-click a
      bar to change its signature for the selected track

### Project hygiene
- [x] Resolved the pre-existing clippy warnings; `cargo clippy --all-targets -- -D warnings` now passes.

---

## 13.08.2026 ✅ Implemented / Fixed this pass

### Explicit bar count (foundation for everything below)
- [x] `InstrumentTrack` now has a real `bar_count: u32` field instead of an
      implied, ever-expanding render horizon. Added `bars_end_tick()` and
      `bar_index_at(tick)` helpers. `BwxProject::normalize_bar_counts()`
      backfills `bar_count` after importing a `.tg` file (defensive against a
      malformed foreign import); `.bwx` always writes its own `bar_count` and
      requires it on load — no back-compat shim, since there's no prior
      `.bwx` release to be compatible with.
- [x] `horizon_tick` and the bar-grid renderer now stop at each track's real
      `bar_count` instead of drawing bars indefinitely — the canvas now
      reflects the project's actual shape, and a bar has to be added before
      you can write past the current last bar.
- [x] `.tg` export (`build_headers`) now derives its measure count from
      `bar_count` (with a note-content safety net) instead of note-derived
      padding.

### Bar editing — RMB menu + shortcuts
- [x] Insert Bar (to the left of selected bar) (`Ctrl+Ins` and `Ctrl+Shift+B`)
- [x] Add a Bar (to the right of selected bar) (`Ctrl+B`, also on the palette's `Bar+` button)
- [x] Delete Bar (`Ctrl+Del`) — refuses to remove a track's last bar
- All three operate per-track (bars are per-instrument/polymetric), are full
  undo/redo steps, and correctly carry/shift time-signature changes across
  the edit (tested in `core/edit.rs`).

### Starting menu
- [x] New landing screen shown at launch (and via File → Start Menu... / the
      toolbar's Home button): a vertical **recent projects** list
      (double-click to open), an **Open other project...** button, a
      **New Project...** button, and a **Demo Project** button (this is what
      used to auto-open on startup).
- [x] Recent-projects list is persisted to a small text file next to the
      executable and updated on every open/save.
- [x] **New Project dialog**: name field (defaults to "Untitled", auto
      de-duplicated against recent projects as "Untitled 1", "Untitled 2", …
      — same de-dup applied to whatever name is typed), first-track
      instrument family picker (Stringed / Orchestra / Drums / MIDI — only
      Stringed is functional, the rest are shown disabled with a "Coming
      soon" tooltip per the plan), Stringed sub-type picker (Acoustic
      Guitar / Electric Guitar / Bass / Other), and a string-count picker
      (quick buttons up to 12 plus a free-form `DragValue` for any count).
      New projects are empty: 1 track, 1 bar, 4/4, 120 bpm.
- [x] String tuning generator (`StringedVariant::default_tuning`) supports
      any string count, extending in perfect fourths below the standard
      tuning for extended-range instruments (7-string low B, 8-string low
      F#, 5-string bass low B, etc.) and keeping the highest strings for
      reduced counts.

---

## 14.08.2026 ✅ Implemented / Fixed this pass

### Staff layout: no more silently-split bars, no more phantom staff lines
- [x] Rewrote the staff layout from "fixed tick-width rows" to real system
      packing (`StaffSystem`, `BetterWriterApp::layout_staff_systems`): each
      row is a contiguous run of *whole* bars sized to fit the available
      width. A bar that wouldn't fit in the remaining space of a row no
      longer gets silently cut in half — the whole row ends and that bar
      starts the next line instead.
- [x] Staff/tab horizontal lines now stop at the last bar's actual right
      edge for that row (with a closing "wall" line), instead of stretching
      all the way to the page margin regardless of content.
- [x] `paint_selected_track_page`, `handle_canvas_click`, and
      `paint_cursor_cell` all now share this one layout so hit-testing,
      the cursor cell, and what's drawn can never disagree.

### Tempo automation, replacing the single global `tempo_bpm`
- [x] `BwxProject.tempo_bpm: f32` is gone, replaced by a real timeline:
      `tempo_points: Vec<TempoPoint>` (`at_tick`, `bpm`, `transition:
      Constant|Progressive`, `hidden`), with `tempo_at`/`interpolated_tempo_at`
      queries and `set_tempo_point`/`delete_tempo_point`/
      `set_tempo_point_hidden`/`clear_tempo_automation` mutations (all
      unit-tested in `core/edit.rs`).
- [x] **Playback is tempo-map-aware now, not just the UI**: `audio/mod.rs`
      precomputes constant-rate segments from the automation timeline
      (`build_tempo_segments`) and walks them incrementally during playback.
      `Progressive` segments use their average bpm as the segment rate — a
      good approximation for scheduling; the graph still shows the exact
      linear ramp. True sample-accurate ramped playback is still open (see
      below).
- [x] `.tg` import/export now reads/writes real per-measure tempo points
      instead of a single flat value (mirrors the existing `signature_changes`
      pattern via a new `tempo_points()` helper in `tuxguitar.rs`).
- [x] On-staff labels: a non-hidden tempo point draws a "♩=bpm" tag directly
      above its tick position on the staff (`paint_tempo_labels`), matching
      Guitar Pro's automation display.
- [x] **Automation Editor** window (toolbar button, "TVP Automation" —
      replaces the old flat tempo `DragValue`), modeled on Guitar Pro 8's:
      TYPE dropdown (Tempo implemented; Volume/Pan shown disabled with
      "Coming soon", ready for when per-track automation exists), a POINT
      SETTINGS panel (tempo value + TAP-tempo button, Constant/Progressive
      transition, per-point "hide label" toggle, delete), a "Remove
      automations" reset, and a scrollable/zoomable graph
      (`egui::ScrollArea::both`, independent zoom slider, snap-to-grid) with
      bar gridlines from the selected track.
      - Points are **draggable** (moves tick + bpm together) and
        **area-selectable**: drag from empty canvas to rubber-band select
        several points, then drag or delete them as a group. The first point
        (tick 0) can have its bpm dragged but not its tick, and can't be
        deleted — playback always needs a starting tempo.
      - Clicking empty canvas adds a new point there; clicking a point
        selects it; each drag gesture (move or area-select) is a single
        undo step.

### Start Menu
- [x] The recent-projects list now fills all the way to the bottom of the
      window instead of stopping at a fixed height.
- [x] "Open other project..." moved above the list, next to a new
      **Search** box.
- [x] Typing in Search *re-sorts* (never filters/hides) the recent list by
      match quality: primarily by how many query characters matched *in the
      same order* as typed (a subsequence/fuzzy-finder score), then by raw
      character overlap as a tiebreaker (`search_match_score`).

---

## 15.08.2026 ✅ Implemented / Fixed this pass

### Build warnings cleaned up
- [x] Fixed all 3 warnings from the first real `cargo build`: an unused
      `TempoPoint` re-export (now actually used via the re-exported path
      instead of a fully-qualified one in `edit.rs`/`tuxguitar.rs`),
      `AudioCommand` being `pub` with no external callers (it's only ever
      used inside `audio/mod.rs`, so it — and `TempoSegment` alongside it —
      are now private), and `interpolated_tempo_at` being dead code (now
      used for a live "effective tempo at cursor" readout in the Automation
      Editor when no point is selected).

### Root-caused the floating/overlapping note-block rendering
Two screenshots showed a note's tab-block either floating past a bar's right
wall, or visually overlapping the start of the next bar's content. Root
cause: `insert_note` placed notes with whatever duration was selected,
with no check that the result was musically representable — a note could
end up spanning past the bar it started in, or overlapping another note
already sounding on the same string. That invalid data was then what was
being rendered, which is what actually broke.
- [x] `BwxProject::insert_note` now shrinks the note's duration as needed
      instead of ever creating that impossible state:
      - it's capped to end by the boundary of the bar it starts in (this app
        has no cross-bar ties yet, so a note's block always belongs to
        exactly one bar);
      - if a later note already exists on the *same string*, the new note is
        capped to stop right before it;
      - if an earlier note on the *same string* already sustains into where
        the new note starts, that earlier note is shortened to stop there.
      Notes on different strings (chords) are completely unaffected by any
      of this. Covered by four new unit tests in `core/edit.rs`.
      Since note data can no longer cross a bar boundary at all, the
      originally-proposed "visually teleport the overflow into the next bar"
      idea turned out to be unnecessary — fixing the data at its source
      means there's never an overflow to render in the first place.
- [x] Added a small defensive clamp on top regardless: a note block's
      rendered width is now also capped at the row's right wall, so even a
      hypothetical edge case (e.g. old data from before this fix) can't
      visually poke past it.

---

## 16.08.2026 ✅ Implemented / Fixed this pass — revisiting 15.08.2026

Turns out the 15.08.2026 fix above was too heavy-handed: clamping a note's
duration to its bar boundary also killed a workflow that was actually useful
— stretching a note past a bar to represent a tied note, ahead of real tie
support. It's also exactly the representation planned for a future
tab-block ↔ classic-notation view toggle (tied notes joining into one block
in block mode, and splitting back into the minimal number of tied notes when
switching to classic tab view). So the bar-boundary clamp is reverted; only
the *same-string overlap* prevention from that pass stays.

- [x] `BwxProject::insert_note` no longer clamps a note's duration to the
      bar it starts in — a note can once again legitimately span past a bar
      boundary. Same-string overlap prevention (shrinking whichever note
      would otherwise overlap another on the same string) is unchanged and
      still applies regardless of bar boundaries. Updated/added unit tests
      in `core/edit.rs` to match (`insert_note_can_span_past_a_bar_boundary`
      replaces the old bar-clamp test).
- [x] Actually built the originally-proposed visual fix this time: a note
      whose span crosses a system/row's right wall now renders as multiple
      **fragments** (`BetterWriterApp::paint_note_fragment`,
      `paint_note_block`) — the piece in each row is clipped to that row,
      with the border **open** on whichever side it continues through (no
      right border if it keeps going into the next row, no left border on
      the continuation piece that picks up at the start of that row). Only
      the very first (onset) fragment carries the standard-notation
      dot/stem, the fret digit, and effect labels like "P.M." — continuation
      fragments are a bare connecting block, so the whole thing reads as one
      continuous sound rather than a second note. Works for a note spanning
      any number of rows, not just one.

### Known inconsistency to revisit
- [ ] Growing an *existing* note's duration via `Shift+→`
      (`set_note_duration_fluid`) still refuses to extend past its bar,
      unlike a freshly-typed long note (which can now span bars again, per
      above). Left as-is for now since that function's "shift/delete
      subsequent notes to make room" logic gets meaningfully more complex
      once resizing is allowed to reach into a neighboring bar's own,
      unrelated notes — worth a dedicated pass, likely alongside the planned
      tied-note / block-mode toggle work described above.

### Automation Editor: window wasn't resizable in height
- [x] Fixed: the window's content wasn't claiming the full space a manual
      resize gave it, so egui shrink-wrapped the height back to the
      content's natural (small) size every frame — width happened to work
      because the graph's content width already usually exceeded the
      window's width. `show_automation_editor_contents` now claims full
      available height explicitly up front.
- [x] Bonus: the graph itself now grows to fill the extra space when the
      window is resized taller (previously a fixed 280px regardless of
      window size).

---

## 17.08.2026 ✅ Implemented / Fixed this pass

Seven separate items from one round of testing feedback.

### 1. Automation Editor: graph was tiny; transition controls hidden; layout wasted space
- [x] **Root cause of the tiny graph**: `ui.allocate_ui(fixed_size, ...)` for
      the point-settings column inherited the *parent* `ui.horizontal`'s
      left-to-right layout, which disables text wrapping — the point-
      settings text was spilling out sideways past its column instead of
      wrapping, which visually squeezed the graph column down to almost
      nothing. Fixed with `allocate_ui_with_layout(..., Layout::top_down(...))`
      for both columns, so each one properly wraps/stacks within its own
      width. The graph now also takes an explicit height budget (measured
      from an actual drawn row height, not a possibly-wrong
      `ui.available_height()` read from inside the broken layout) and fills
      it.
- [x] **Constant/Progressive is now always visible**, not nested inside a
      "exactly 1 point selected" branch. It's a persistent "brush": it never
      resets itself to reflect whatever's currently selected, clicking it
      applies to the current selection (if any), and it's what new points
      (created by clicking empty graph space) are created with.
- [x] **Reorganized POINT SETTINGS into one compact, always-visible vertical
      column**: Position → Tempo (+ TAP) → Transition → Hide automation
      label → Delete, each field showing sensible placeholder/disabled
      state when 0 or >1 points are selected instead of the whole panel
      changing shape depending on selection count.

### 2. Split note blocks: the "teleported" fragment wasn't clickable
- [x] Clicking a note's continuation fragment (the part that "teleports" to
      the next row) now correctly selects the underlying note, instead of
      being treated as empty space — which is what let a new note get typed
      right on top of it. Refactored the hit-test into a shared
      `find_note_fragment_at` helper used by both click-to-select and (new
      this pass) drag-start, so onset and continuation fragments are always
      handled identically everywhere the canvas does hit-testing.

### 3. New: area-selection for note blocks
- [x] Dragging from empty canvas space now rubber-band selects every note
      whose visible fragment (onset or continuation, in any row) intersects
      the rectangle — mirroring the Automation Editor's point selection.
      Selected notes highlight with the accent color. `Delete`/`Backspace`
      deletes the whole selection as one undo step; the toolbar's `Dur`/`Del`
      buttons and the RMB Note ▸ Duration submenu now also operate on an
      area-selection when there's no single note selected.
- [ ] Not implemented: batch-moving an area-selection together (only
      selection + batch delete + batch duration change). Worth adding if it
      turns out to be wanted.

### 4. Bug: RMB duration change deleted every note in every later bar
- [x] `set_note_duration_fluid` ended with `track.notes.retain(|note|
      note.abs_tick < measure_end)` — applied to the *entire track*, not
      just the notes actually affected by the resize. Any duration change
      silently deleted every note in every bar after the one being edited.
      Removed the line; added a regression test
      (`fluid_duration_does_not_touch_notes_in_later_bars`).

### 5. New: drag-to-stretch a note block's left/right edge
- [x] `BwxProject::stretch_note` (core/edit.rs): dragging the **right** edge
      changes duration only; dragging the **left** edge moves the start tick
      while keeping the *end* tick fixed. Both clamp against the bar
      boundary and against overlapping another note on the same string,
      same spirit as `insert_note`. Snaps to a configurable step (default
      1/16 note). Four new unit tests cover both edges, the same-string
      clamp, and the bar-boundary clamp.
- [x] UI: grab a **selected** note's true left/right edge (not the open side
      of a row-wrap fragment — there's no real boundary there) to start a
      drag; a live preview line + tick-delta label shows the snapped result
      while dragging, applied as a single undo step on release. Cursor
      changes to a resize icon when hovering a grabbable edge.
- [x] Step size is configurable via RMB ▸ Note ▸ Stretching: quick presets
      (1/32 through whole note) plus a free-form tick value, mirroring the
      picker style already used elsewhere (New Project's string-count
      picker).
- [ ] Known same limitation as item 4's neighbor: stretching (like resizing
      via `Shift+→`) can't cross a bar boundary. Consistent with existing
      behavior, but worth revisiting alongside the tied-note/block-mode work.

### 6. Bug: time signature denominator submenu wouldn't open
- [x] Root cause: an `egui::ComboBox` nested inside a `ui.menu_button` — a
      known egui conflict, since both are popup-based and a click on the
      inner one gets read as "clicked outside the outer menu", closing it.
      Replaced with a row of selectable buttons (already the established
      pattern elsewhere in this app, e.g. the New Project dialog's
      string-count picker).

### 7. Time signature labels repeated on every bar
- [x] A bar's time signature label now only draws where the signature
      actually *changes* (plus always at the very first bar) — not on every
      subsequent bar that happens to share the current signature.

---

## 18.08.2026 ✅ Implemented / Fixed this pass

Another round of testing feedback, seven items plus a build warning.

### Build warning
- [x] Fixed unused `system_ticks` in `handle_canvas_click` (dead after an
      earlier refactor stopped needing it there).

### 1. Automation Editor
- [x] **Delete/Backspace deletes the selected point(s)**, as long as no
      widget (e.g. the bpm field's text-edit mode) currently has keyboard
      focus, so backspacing a typed number doesn't also delete the point.
- [x] **Fixed the snap-to-grid "runs away to 0" bug.** Root cause: dragging
      recomputed each point's new position from its *already-snapped*
      previous-frame position every frame, so a small nudge could get
      re-snapped to the same grid line repeatedly (no visible movement)
      until it suddenly jumped a whole grid cell — compounding badly.
      Rewrote it as an **elastic snap**: `AutomationDragMode::MovingPoints`
      now stores every selected point's position *exactly as it was when
      the drag started*, plus the total raw (unsnapped) drag distance
      accumulated since then. Each frame recomputes fresh from that fixed
      baseline — only the grabbed point's raw distance from a grid line
      decides whether the whole selection is currently snapped, the
      threshold is distance-based (in pixels, so it feels consistent at any
      zoom level) rather than "did we cross a cell boundary", and moving far
      enough from a grid line naturally "unsnaps" back to the raw cursor
      position, exactly as described. Same technique reused for note
      dragging (item 6 below).
- [x] **Right-click on empty graph space clears the selection.**

### 2. Note stretching
- [x] **Can now cross a bar boundary**, consistent with `insert_note` — this
      app's stand-in for a tied note until real ties exist. Test updated
      (`stretch_note_right_edge_can_cross_a_bar_boundary` replaces the old
      "cannot cross" test).
- [x] **Multiple selected notes stretch together.** Grabbing the edge of any
      note that's part of the current area-selection now stretches every
      selected note's matching edge by the same amount, as one undo step.
      Grabbing an edge outside the selection still just stretches that one
      note.
- [x] **Bigger, consistent hitbox.** `EDGE_GRAB_PIXELS` raised from 5px to
      10px, and — this was really the actual bug — the resize-cursor
      indicator and the real grab now both call the exact same
      `stretch_edge_at`, so the cursor changing to `<-->` and being able to
      actually grab there can never disagree again.

### 3 & 4. Time signature denominator and note duration: list-scrolling pickers
- [x] Both were converted to a new `list_drag_value` helper: a `DragValue`
      bound to a *list index* with a `custom_formatter` mapping it to the
      musically-meaningful label — same drag-to-change / click-and-type feel
      as a normal `DragValue` (like the numerator), but stepping through
      {1,2,4,8,16,32} or the six note durations instead of every integer.
      Replaces the denominator's row-of-buttons workaround from 17.08.2026
      (still correct, just slower to use) and the toolbar's duration
      `ComboBox`.

### 5. Auto-commit note entry (no Enter needed)
- [x] Typing a fret digit now auto-commits ~0.3s after the last digit,
      matching Guitar Pro — enough time to type a two-digit fret in one go
      without needing Enter. Implemented as `auto_commit_pending_fret`,
      called every frame (via `handle_keyboard`) and using
      `ctx.request_repaint_after` to make sure a repaint actually happens
      right at the 0.3s mark even if the user does nothing else in the
      meantime (egui doesn't repaint on a timer by default). Also fixed a
      bug this surfaced: `commit_fret_at_cursor` cleared `pending_fret` but
      not `pending_at`, which would have made every commit path (not just
      auto-commit) immediately re-fire on the next frame using the default
      fret.

### 6. New: dragging a note to move it in time
- [x] `BwxProject::move_note` (core/edit.rs): shifts a note's start tick,
      duration unchanged, clamped against overlapping another note on the
      same string (excluding whichever other notes are moving together in
      the same gesture, so a multi-note drag doesn't clamp against itself).
      Free to cross a bar boundary. Three new unit tests.
      UI: dragging the *body* (not an edge) of an already-selected note
      moves it — and the rest of the selection, if it's part of one — using
      the same elastic-snap technique as item 1, but anchored to **the bar
      the grabbed note started in** rather than tick 0, per spec. Holding
      **Shift disables snapping** entirely. Live preview line while
      dragging; step size configurable via the new RMB ▸ Note ▸ Dragging
      submenu (same preset/custom picker as Stretching), right below it.

### 7. RMB context menu: submenus were closing on every click inside them
- [x] Root cause: egui's menu popups default to
      `PopupCloseBehavior::CloseOnClick` — closing on *any* click, including
      on a widget inside the menu, not just outside it. That's what made
      the Stretching submenu (and every other interactive submenu control)
      effectively unusable. Replaced every `ui.menu_button` in the context
      menu tree with a new `submenu()` helper
      (`egui::containers::menu::MenuButton` + `MenuConfig` with
      `PopupCloseBehavior::CloseOnClickOutside`), and the top-level
      `response.context_menu(...)` with the equivalent `egui::Popup`
      builder. Action items that should still close after being clicked
      (most of them) already had an explicit `ui.close()` call, which still
      works the same regardless of this setting — only items *without* one
      changed behavior, which is exactly the ones that shouldn't have been
      closing the menu in the first place.

---

## 19.08.2026 ✅ Implemented / Fixed this pass

### 1. Note dragging: vertical movement across strings
- [x] `BwxProject::move_note` extended with a `string_delta` parameter —
      dragging a note (or a whole selection, moving together) vertically now
      changes which string it's on, not just when in time. Clamped to a
      valid string index, and against overlapping another note already on
      the *target* string.
- [x] While implementing this, found and fixed a real bug in the existing
      horizontal-only clamping: when the tentative landing spot fell inside
      an obstacle note with no room "before" it (e.g. an obstacle starting
      at tick 0), the old bounding-based clamp could leave the moved note
      sitting exactly on top of the obstacle instead of resolving the
      overlap. Rewrote it to detect genuine overlap at the *landing*
      position and push to whichever valid side (before/after the obstacle)
      is closer, with a validity check for cases where "before" has no room
      at all. Five `move_note` unit tests now, covering both axes and the
      overlap edge case above.
- [x] Live preview and status text now report the string offset alongside
      the tick offset while dragging.

### 2. Theme switching: System / Dark / Bright
- [x] New View ▸ Theme menu with three options. `AppThemeMode::System`
      (the default) follows the OS light/dark preference via
      `ctx.input(|i| i.system_theme)`, falling back to **Dark** if egui
      can't determine it (some platforms/window managers don't report a
      preference) — exactly as requested, not silently guessing Bright.
- [x] `EditorTheme` split into `bright()` (the app's original look — light
      sheet on dark chrome, kept as `Default`) and a new `dark()` variant
      with a genuinely dark sheet background, not just dark surrounding
      panels. Resolved once per frame in `resolve_theme`, which also calls
      `ctx.set_theme(...)` so every built-in egui widget (buttons, windows,
      menus, the Automation Editor, text fields — everything not
      hand-painted on the staff canvas) follows the same light/dark switch
      automatically, not just the custom-painted parts.
- [x] Retrofitted the biggest-impact hardcoded colors onto the theme: every
      panel background (menu bar, toolbar, palette, fretboard, track table,
      status bar, canvas backdrop), and the staff's clef/time-signature
      labels and note onset dot/stem (previously hardcoded black/near-black,
      which read poorly against a dark sheet). Some smaller, self-contained
      accents (e.g. the Automation Editor's graph, which is deliberately
      dark regardless of app theme, like most DAW automation lanes) were
      left as-is — see below.

---

## 🚧 Not yet done / future work

### Current issues:
- [x] ~~Compilation not verified~~ — confirmed building clean with zero
      warnings as of 17.08.2026 and 18.08.2026 (this pass has not yet had
      its own build verified locally, same as every pass in this sandbox —
      see the per-pass notes above).
- [ ] Theme switching (19.08.2026, item 2) covers the biggest-impact colors
      but not every hardcoded one in `app.rs` — smaller accents (e.g. the
      P.M. label, playhead, selection-rectangle colors) weren't audited
      individually. Worth a dedicated pass through the remaining
      `egui::Color32::from_rgb(...)` call sites if the Dark theme turns up
      any more low-contrast spots in practice.
- [ ] Note dragging (19.08.2026, item 1) can change string, but there's no
      equivalent for *stretching* a note's edges across strings (stretching
      only ever changes duration/start on the note's current string) — not
      requested, just noting the asymmetry.
- [x] ~~Batch-moving an area-selection of note blocks together~~ —
      implemented 18.08.2026, item 6 (drag the body of any selected note to
      move the whole selection together).
- [x] ~~Drag-to-stretch can't cross a bar boundary~~ — relaxed 18.08.2026,
      item 2, consistent with `insert_note`/`move_note`. `Shift+→` resizing
      (`set_note_duration_fluid`) is the one place that still can't cross a
      bar boundary — worth a dedicated pass, since its "shift/delete
      subsequent notes" logic gets meaningfully more complex once resizing
      can reach into a neighboring bar's own, unrelated notes.
- [ ] Note dragging (18.08.2026, item 6) only moves notes in time
      (horizontally) — it doesn't support dragging a note to a different
      string (vertically). Not requested yet, but a natural next step if
      wanted.
- [ ] Playback tempo ramps (`Progressive` transition) are approximated by a
      flat average-bpm segment for audio scheduling; the automation graph
      shows the true linear ramp, but sample-accurate ramped playback timing
      is still open. Worth a dedicated pass if precise tempo-ramp audio ever
      matters (e.g. a gradual accelerando).
- [ ] Volume/Pan automation is UI-only scaffolding (disabled options in the
      Automation Editor's TYPE dropdown) — no per-track automation data
      model exists yet.
- [ ] **Future idea (from 16.08.2026 discussion): a block-mode ↔ classic-tab
      view toggle.** Classic mode would render/edit like Guitar Pro/TuxGuitar
      (individual tied notes across barlines); block mode (this app's
      current look) would show a run of tied notes as one single joined
      block. Switching to block mode would merge tied-note runs into one
      block each; switching back to classic mode would split a block back
      into the minimal number of tied notes needed to represent it. Real
      tie support is a prerequisite (right now, a note simply spanning past
      a bar boundary is this app's stand-in for a tie — see
      `BwxProject::insert_note`'s doc comment and the 16.08.2026 section
      above).

### Core notation engine (Phase 2)
- [x] Right mouse click (RMB) in an area to open context menu with options:

- [ ] Cut (`Ctrl+X`)
- [ ] Copy (`Ctrl+C`)
- [ ] Paste (`Ctrl+V`)
- [ ] All-Track Cut (`Ctrl+Shift+X`)
- [ ] All-Track Copy (`Ctrl+Shift+C`)
- [ ] Special Paste... (`Ctrl+Shift+V`)
- [x] Bar
    
    - [x] Insert Bar (to the left of selected bar) (`Ctrl+Ins` and `Ctrl+Shift+B`)
    - [x] Add a Bar (to the right of selected bar) (`Ctrl+B`)
    - [x] Delete Bar (`Ctrl+Del`)
    - [ ] Clef... (`K`)
    - [ ] Key Signature... (`Ctrl+K`)
    - [x] Time Signature... (`Ctrl+T`)
    - [ ] Triplet Feel... (`Ctrl+/`)
    - [ ] Free Time (`|`)
    - [ ] Double Barline
    - [ ] Anacrusis (Pickup Bar)
    - [ ] Repeat Open (`[`)
    - [ ] Alternate Endings...
    - [ ] Repeat Close... (`]`)
    - [ ] Directions... (`D`)
    - [ ] Simile Marks
    - [ ] Multirest (`Ctrl+R`)
    - [ ] Force Break Line (`Ctrl+Return`)
    - [ ] Prevent Break Line
    - [ ] System Layout...
    
- [x] Note
    
    - [ ] Insert a Beat (`Ins`)
    - [ ] Delete the Beats (`Shift+Del`)
    - [ ] Copy Beats at the End (`C`)
    - [x] Duration
    - [x] Dynamic
    - [x] Ghost Note (`O`)
    - [x] Accented Note (`;`)
    - [x] Heavily Accented Note (`:`)
    - [x] Staccato (`!`)
    - [ ] Staccatissimo
    - [ ] Tenuto
    - [ ] Tie Note (`L`)
    - [ ] Tie Beat (`Shift+L`)
    - [ ] Rest (`R`)
    - [ ] Fermata... (`F`)
    - [ ] Accidentals
    - [x] One Semitone Down (`Alt+Shift+Down`)
    - [x] One Semitone Up (`Alt+Shift+Up`)
    - [ ] One Octave Down (`Alt+Shift+PgDown`)
    - [ ] One Octave Up (`Alt+Shift+PgUp`)
    - [ ] Left Hand Fingering...
    - [ ] Right Hand Fingering...
    - [ ] String Number
    - [ ] Shift Down (`Alt+Down`)
    - [ ] Shift Up (`Alt+Up`)
    - [ ] Pickstroke Down (`Shift+D`)
    - [ ] Pickstroke Up (`Shift+U`)
    - [ ] Chord... (`A`)
    - [ ] Scale Diagram... (`Shift+S`)
    - [ ] Text... (`T`)
    - [ ] Timer (`@`)
    - [ ] Slash
    - [ ] Barre... (`Shift+I`)
    - [ ] Octave Sign
    - [ ] Design
    - [ ] Audio Note Settings... (`Shift+F`)
    
- [x] Effects
    
    - [ ] Grace Note
    - [ ] Trill... (`N`)
    - [ ] Ornament
    - [ ] Tremolo
    - [x] Let Ring (`I`)
    - [ ] Sustain Pedal
    - [ ] Legato (`Shift+H`)
    - [x] Hammer On / Pull Off (`H`)
    - [ ] Left Hand Tapping (`(`)
    - [x] Tapping (`)`)
    - [x] Slap (`S`)
    - [x] Pop (`Ctrl+S`)
    - [ ] Dead Slapped
    - [x] Dead Note (`X`)
    - [x] Palm Mute on Note (`P`)
    - [ ] Palm Mute on Beat (`Shift+P`)
    - [ ] Pick Scrape Out Downwards
    - [ ] Pick Scrape Out Upwards
    - [ ] Bend... (`B`)
    - [x] Slide
    - [ ] Tremolo Bar... (`Shift+W`)
    - [x] Vibrato
    - [ ] Vibrato w/ Trem. Bar
    - [ ] Natural Harmonic (`Y`)
    - [ ] Artificial Harmonic... (`Ctrl+Alt+Y`)
    - [ ] Brush Downstroke... (`Ctrl+D`)
    - [ ] Brush Upstroke... (`Ctrl+U`)
    - [ ] Arpeggio Down... (`Ctrl+Shift+D`)
    - [ ] Arpeggio Up... (`Ctrl+Shift+U`)
    - [ ] Rasgueado... (`Shift+R`)
    - [ ] Golpe Finger
    - [ ] Golpe Thumb
    - [x] Fade In (`<`)
    - [ ] Fade Out (`>`)
    - [ ] Volume Swell (`Alt+<`)
    - [ ] Wah Open (`Ctrl+Alt+O`)
    - [ ] Wah Close (`Ctrl+Alt+C`)
    
- [ ] Left mouse drag to select area of notes, both vertically and horizontally, in tab view.
- [ ] **Tempo map** (`tempo_bpm` is project-global today; needs per-tick tempo
      changes for the shadow-timeline playback to be sample-accurate).
- [ ] Ties, grace notes, tuplets, dotted/double-dotted durations.
- [ ] Real chord detection (multiple notes on one tick drawn as a chord block).
- [ ] Instruments adding/deleting
- [ ] Bar adding/deleting
- [ ] Some kind of bar scrolling
- [ ] Vertical bar placement to fill the screen efficiently
- [ ] Brick note length manual editing with pulling it left or right as in piano rolls.
- [ ] Making Virtual guitar neck/Fretboard panel: 1. usable for note input 2. tuning changable. 3. Hideable (Togglable on/off). 4. Guitar nut area isn't displayed properly, there should be area between the note and the nut (E---|(nut)---|(1 fret)---..., --- is clickable area for the nut/fret to its right)
- [ ] In timeline instead of diamonds for each note display a row of blocks with a block for each existing bar, a block with a width of a bar size/time measure (4/4, 3/4, 6/4) relative to all other bar sizes in the project, so 1/4 is 4x smaller than 4/4, but to make them all visible and clickable, the smallest cannot go smaller than some width amount of pixels, and if it gets to that, make everything get bigger. Each instrument has its own row of bar blocks.
- [ ] Starting menu with recently opened projects kept in a vertical list, double-clicking opens the project to the edit menu we currently have. There should be a button to create new project with options: 1: name ("Untitled" by default, if it already exists add 1 then 2, etc.), 2. First track instrument Currently only Stringed will work, but also add Orchestra, Drums, MIDI. Stringed has options of Acoustic guitar, Electric guitar, Bass, Other. In the future each will have default playback sound that will be modified with VST3s. 3. Amount of strings selection (up to 12, but in the input menu make any value above inputable.) Add there a button for "Demo project" option will open what we currently have as open-on-startup. New projects are fully empty and only have 1 instrument and 1 bar at 4/4 set to default tempo of 120 bpm.
- [ ] Make instruments selectable to display multiple of their tabs/notations at the same time (current time of each instrument's notes is the key for synchronization here)

### Audio / playback (Phase 3)
- [ ] **Live playback playhead** rendered on the canvas synced to the audio
      callback (the engine plays, but there is no moving cursor yet).
- [ ] Transport scrubbing: rewind / fast-forward / jump-to-start actually move
      the playhead (buttons exist but are no-ops).
- [ ] Loop regions, metronome, count-in.
- [ ] Verify `oxisynth` actually renders `MS Basic.sf3` audibly (stream is
      wired but untested end-to-end on this machine).
- [ ] Channel/program-per-track: each track's `program` should drive its MIDI
      channel's patch on the synth (currently all channels forced to program 24).
- [ ] Real **VST3 hosting** via `vst3` crate (the `Vst3HostSlot` is currently a
      pass-through stub that only records loaded paths). Requires implementation of VST3 folder scanning (`C:\Program Files\Common Files\VST3`).

### File formats (Phase 4)
- [x] **Native `.bwx` save/load round-trip** tested through the UI. Works good at the state of 19.06.2026
- [ ] **`.tg` import/export** beyond the current TuxGuitar subset reader. Export can't be tested, blocked due to inability to delete test instrument with polythithms
- [ ] **`.gp` (Guitar Pro)** reader/writer (export path is a placeholder text
      bundle today).
- [ ] Multi-track bundle export wizard with a per-track checklist (currently
      exports all tracks unconditionally).

### UI / UX
- [ ] **Custom theme palette editor** (`EditorTheme` exists but is not
      user-editable at runtime).
- [ ] Scrollable / paginated score view beyond the 8-system cap.
- [ ] Note effect glyphs rendered on the staff (palm-mute text exists; others
      are modeled but not drawn).
- [ ] Drag-to-move notes; drag-select range.
- [ ] Classic notation view above tablature should be toggleable on/off. Also classic notation mode shows all note lengths as a 1/4th note.
- [ ] One-button mode switching from brick painting to UI similar to Guitar Pro, TuxGuitar, etc. with fret numbers instead of bricks and intelligent rests filling at the end of bars. Upon switching, bricks that lay between bars should be calculated as tied notes, with its length naturally spread across bars as tied notes. when switching back to brick mode they such tied notes should be combined into full bricks again. (only ones that were created as bricks. Custom user's tied notes should be considered separate notes but connected with sound.


Info for left menu:
Here are the official names for all the notation symbols and functions found on the left Edition Palette of Guitar Pro 8, broken down section by section from top to bottom.
## Section 1: Bar, Time Signature, and Repeats

- Row 1:
    
    - `Clef` — Changes the musical clef (Treble, Bass, Alto, etc.)
    - `Key Signature` — Sets the accidental key signature (sharps/flats)
    - `Time Signature` — Changes the meter (e.g., 4/4, 3/4)
    - `Triplet Feel` — Applies a shuffle or swing rhythm feel to the playback
    - `Bar Line` — Inserts a custom bar line type (dashed, double bar, etc.)
    - `Final Bar Line` — Marks the very end of the score or piece
    - `Repeat Close / Simile (1 bar)` — Repeats the previous 1 measure
    - `Simile (2 bars)` — Repeats the previous 2 measures
    
- Row 2:
    
    - `Repeat Open` — Marks the starting point of a repeated section (highlighted in blue)
    - `Directions` — Inserts navigation markers (e.g., Coda, Segno, Fine, x.)
    - `Repeat Close` — Marks the ending point of a repeated section (highlighted in blue)
    - `Da Capo / Al Coda symbols` — Advanced repetition directions and placeholders
    - `Ottava Alta (8va)` — Play notes one octave higher than written
    - `Ottava Bassa (8vb)` — Play notes one octave lower than written
    - `Quindicesima Alta (15ma)` — Play notes two octaves higher than written
    - `Quindicesima Bassa (15mb)` — Play notes two octaves lower than written
    

---

## Section 2: Note Durations, Accidentals, and Dynamics

- Row 1 (Durations): Whole note, Half note, Quarter note (highlighted), Eighth note, Sixteenth note, Thirty-second note, Sixty-fourth note.
- Row 2 (Modifiers):
    
    - `Augmentation Dot` — Adds half of the note's value to its length
    - `Double Augmentation Dot` — Adds 75% of the note's value
    - `Tuplet` — Groups notes into standard divisions (like triplets)
    - `Nested Tuplet (Primary)` — Guitar Pro 8's dedicated button for managing top-level tuplets
    - `Nested Tuplet (Secondary)` — Manages tuplets contained inside other tuplets
    - `Tie Note` — Connects the duration of two identical pitches
    - `Tie Beat` — Links entire beats together
    - `Fermata` — Instructs the note or rest to be held longer than its standard value
    
- Row 3 (Accidentals): Flat, Double Flat, Natural, Sharp, Double Sharp, Quarter-Tone Flat, Quarter-Tone Sharp.
- Row 4 & 5 (Dynamics):
    
    - Dynamic Markings: _ppp_ (pianississimo) to _fff_ (fortississimo)
    - `Crescendo` (<) — Gradually get louder
    - `Decrescendo / Diminuendo` (>) — Gradually get softer 
    

---

## Section 3: Performance Techniques (Guitar & General)

- Row 1: Parentheses (Ghost Note), Dead Note (X notehead), Grace Note before beat, Grace Note on beat, Accent, Heavily Accented Note, Let Ring, Sustain Pedal, Palm Mute.
- Row 2: Natural Harmonic, Artificial Harmonic, Bend, Tremolo Bar, Slide Into, Slide Out Of, Legato Slide, Shift Slide.
- Row 3: Vibrato (Left Hand), Wide Vibrato (Tremolo Arm), Hammer-On / Pull-Off, Left-Hand Tapping, Slap, Pop, Left Hand Mute, Right Hand Mute, Pick Scrape.
- Row 4: Downstroke (Picking direction), Upstroke (Picking direction), Rasgueado, Arpeggio Down, Arpeggio Up, Pull-off / Hammer-on variation, Upper Mordent, Lower Mordent.
- Row 5: Left Hand Fingering, Right Hand Fingering, Trill, Tremolo Picking, Extended Vibrato, Turn, Inverted Turn.
- Row 6: Left-brace marker, Right-brace marker, Slur bracket, Microtonal/Pitch bend indicator, Slap/Pop modifier, Tapping modifier, Dead Note modifier.

---

## Section 4: Chords, Automation, Texts, and Layouts

- Row 1:
    
    - `Chord Diagram` — Opens the chord editor and gallery
    - `Slash Notation` — Toggles standard slashes for rhythm sheets
    - `Barre Indicator` — Inserts barre fingering lines for stringed instruments
    - `Section Marker` — Places structural rehearsal letters (e.g., A, B, C)
    - `Tempo / Metronome` — Adjusts the track's playback speed
    - `Text` — Adds custom annotation text to the score
    - `Design Mode` — Layout tool to lock, move, or clean up spacing manually
    
- Row 2 (Beams Grouping): Auto-beam configuration buttons to manually break, bind, or extend rhythmic note stems.
- Row 3 (Track Automations):
    
    - `Tempo Automation` — Programs gradual or immediate speed shifts
    - `Volume Automation` — Sets mixing changes directly over the timeline
    - `Pan Automation` — Controls left-to-right stereo movement 
    
