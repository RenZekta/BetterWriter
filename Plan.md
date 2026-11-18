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
- [x] **Insert / overwrite at the cursor**: `Enter` places the buffered fret
      (or the toolbar default) at the cursor; if a note already occupies that
      cell its fret is replaced instead of stacked. (`Space` is the play/stop
      shortcut since 12.11.2026.)
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
      TYPE dropdown (Tempo, Volume, Pan since 19.11.2026), a POINT
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

## 10.11.2026 ✅ Implemented / Fixed this pass

### 1. Classic notation view: toggleable
- [x] The five-line classic notation staff above the tablature can be shown
      or hidden from the toolbar's **Notation** button or View ▸ Classic
      notation. Hiding it moves the tab staff up and centers the time
      signature on the tab; the staff, clef and onset heads/stems are simply
      not drawn.
- [x] First version drew every note as a quarter note. Replaced on
      11.11.2026 by real durations and pitches (see below).
- [x] System layout (`SystemMetrics`) is computed from the toggle and the
      track's string count instead of hard-coded constants, so painting,
      click hit-testing, area selection and edge detection all agree on
      where the tab staff is. A 6-string layout with the staff shown is
      pixel-identical to before; 7+ strings no longer crowd the next system.

### 2. One-button brick view ↔ tab view (Guitar Pro style)
- [x] Toolbar **Tab** / **Bricks** button (and View ▸ Notes) switches
      `EditorViewMode`. Tab view draws bare fret numbers on the strings with
      a backing patch so the string doesn't strike through them; selection,
      the edit cursor and effect labels (P.M.) work as in brick view.
- [x] **Real tie support** in the model: `TabNote::tied_from_previous:
      Option<TieOrigin>`, with `TieOrigin::BrickSplit` (a slice cut from a
      brick) or `TieOrigin::User` (a tie the user wrote). A tie only counts
      while a matching predecessor exists (same string, same fret, ending
      exactly where the note starts) — `InstrumentTrack::is_tied_continuation`
      — so deleting or retuning one slice never leaves a phantom tie.
- [x] **Switching to tab view** (`BwxProject::split_bricks_at_barlines`):
      every brick crossing a barline is cut at each barline of its own
      track (polymetric bars and time signature changes respected) into
      `BrickSplit` tied notes; slices past the last bar extend `bar_count`.
      Shown as `7 ⌒ (7)` with the tie arc, also on the classic staff.
- [x] **Switching back** (`BwxProject::merge_brick_ties`): only
      `BrickSplit` slices are re-joined into the original brick (the unit
      test checks the note comes back byte-identical). `User` ties stay
      separate notes that merely sound continuously; in brick view they get
      a small arc over the seam.
- [x] The switch is not an undo step of its own. Undo/redo, project load and
      every edit in tab view re-normalize for the active mode (both
      directions are idempotent), so history stays consistent across
      switches and a note lengthened past a barline in tab view is split
      immediately (`refresh_after_edit`).
- [x] **Intelligent end-of-bar rests** (`tail_padding_rests`,
      `split_rest_gap`): rests only fill from the last sounding note to the
      barline; chunks are aligned to the bar grid (a gap starting on beat 2
      of 4/4 is a quarter then a half, not a half straddling the middle); a
      note sustaining in from an earlier bar counts as sounding; an empty
      bar is one bar-length rest in any time signature. Tab view draws
      whole/half/quarter/eighth-and-shorter rest glyphs.
- [x] Rests are generated for **every bar** in both views — before this pass
      only the first bar of each row got any.
- [x] **Tie Note (`L`)** (RMB ▸ Note ▸ Tie Note, or `L`): ties the selected
      note(s) to the one right before them, or removes an existing tie.
      Changing a fret (typing over it, `Alt+Shift+Up/Down`) changes the whole
      tie chain, so ties never break by accident (`set_note_fret`).
- [x] Playback treats a tie chain as one sustained note: a continuation emits
      no NoteOn and its predecessor no NoteOff (`ShadowTimeline`), so a
      brick and its tab-view split chain compile to identical events.
- [x] `.tg` import reads the tied flag as a `User` tie and export writes it.
- [x] Moving and stretching notes are disabled in tab view (fret numbers
      have no edges, and dragging a slice would break its chain); selecting,
      typing, deleting and tying work.
- [x] Comment cleanup per the `finding-and-fixing-bad-comments` conventions:
      removed history narration from seven comments in `app.rs`,
      `core/edit.rs`, `core/project.rs` and `format/tuxguitar.rs`, and
      updated `insert_note`'s doc, which still called a bar-spanning note
      "a stand-in for a tied note until real tie support exists".
- [x] Verified: 44 unit tests pass (new: bar splitting incl. polymetric and
      past-last-bar, brick round-trip, `User` ties kept, rest alignment and
      per-bar padding, tie-aware playback, chain-wide fret change). The UI
      was also driven under Xvfb: tab/brick switch, tie across a barline,
      rests and the Notation toggle render as described above.

## 11.11.2026 ✅ Implemented / Fixed this pass (TuxGuitar-informed)

Reference: TuxGuitar's `TGMeasureManager` (rests are real beats that fill
every gap; adding a note replaces a rest and the remainder is re-filled with
the "preferred" duration; a note that doesn't fit the bar is rejected),
`TGBeatGroup`/`TGNoteImpl` (stem direction and beams, ledger lines, per-bar
accidentals) and `TGMusicKeyUtils` (staff position from the sounding pitch).

### 1. Classic notation: real pitch and real durations (`src/notation.rs`)
- [x] **Pitch placement**: staff position now comes from the sounding MIDI
      key (string tuning + fret), written an octave higher as guitars are
      (open high E in the top space, first fret on the top line), with a
      small "8" under the clef. Basses use a bass clef.
- [x] **Accidentals**: black keys are spelled as sharps and remembered for
      the rest of the bar (a repeated F# gets one sharp; F after F# gets a
      natural).
- [x] **Durations**: whole/half (hollow heads), quarter and shorter (filled),
      dots, stems pointing away from the middle line, flags, and beams
      grouped per beat (three eighths in 6/8, 9/8, 12/8, 15/8). Chords
      share one stem; heads a second apart sit on opposite sides.
- [x] Ledger lines above and below the staff, rests on the staff (whole,
      half, quarter, eighth and shorter), ties as arcs on the staff.
- [x] More vertical room per system for ledger lines and stems; tempo labels
      moved clear of them; the opening time signature in brick view now sits
      in front of the staff instead of on top of the first note.

### 2. Tab view works like TuxGuitar instead of like bricks
- [x] **Beat-based layout**: columns are spaced by beat length
      (sub-linearly), not by a fixed pixels-per-tick, and every full row is
      stretched to the page width. Bars with a time signature label get
      extra room. `StaffSystem::x_of` / `tick_at` map ticks to x for
      painting, hit-testing, clicking and the cursor in both views.
- [x] **Beat cursor**: `←`/`→` jump from beat to beat (notes and rest
      slots) instead of stepping by a duration; clicking snaps to the
      nearest beat; the cursor is a compact box on the fret position, not
      a duration-wide brick.
- [x] **Note entry rules** (`commit_fret_at_cursor`): a note in an empty slot
      takes the duration being entered; a note joining a beat that already
      has notes takes that beat's length (chords stay aligned); a note that
      would run past the barline or into the next beat is shortened to the
      longest standard length that fits, and the status line says so (tie
      with `L` to sustain across). After writing, the cursor moves to the
      next beat.
- [x] **Rests fill every gap** in tab view and on the staff
      (`BwxProject::bar_rests`, `RestFill::AllGaps`): a gap is cut into rests
      of the duration being entered where they fit the bar grid, then
      aligned standard values; an empty bar is one bar-length rest. Brick
      view keeps end-of-bar rests (`RestFill::BarEnd`).
- [x] Tests: 67 unit tests pass (new: pitch spelling, clefs, note values,
      beams, stems, accidentals, rest filling policies, `StaffSystem`
      mapping, system metrics). Driven under Xvfb: staff pitches and ledger
      lines, beams, ties across barlines, entry shortening, justified
      multi-row layout, the 5/8 track and brick view.
- [x] Comment pass: all new comments describe current behavior; no history
      narration added.

### 3. Writing conventions cleanup
- [x] Comments and UI strings are ASCII-only: em dashes and `→` replaced. The
      `♩` in "♩=bpm" labels is notation, not punctuation, and stays; AGENTS.md
      now records that exception.
- [x] The tie notes in `core/edit.rs` and the bundle note in `format.rs`
      describe the shipped tie model and the flattened text bundle, instead of
      a stand-in awaiting real ties.
- [x] `InstrumentFamily::is_implemented` and `StringedVariant` docs no longer
      cite the plan or future VST3 patch loading; the `Bright` theme doc drops
      its pre-theme-switching history.
- [x] Past-tense bug narration in `app.rs` (automation window height,
      `top_down` wrapping, nested submenu close behavior, recent-project
      search ranking) rewritten as present-tense egui constraints.
- [x] README: personal background paragraph removed.

## 12.11.2026 ✅ Implemented / Fixed this pass

### 1. Playback no longer freezes the window on the first Play
- [x] **Soundfont loads off the UI thread.** The 51 MB `MS Basic.sf3` used to
      be decoded inside the first Play click; in a debug build that blocked
      the window for ~56 s on one CPU core. The loader now runs on its own
      thread (`AudioRuntime::preload` starts it when the editor opens), the
      stream plays silence until the synth arrives, and a Play pressed while
      loading starts as soon as it is ready (the status bar says so).
- [x] **Decoder panics are contained.** The `soundfont` parser panics on a
      malformed file (covered by a test with a corrupt `.sf3`); the loader
      catches it and reports a normal error. The audio callback is wrapped
      the same way, and the engine rebuilds itself on the next Play after a
      fault.
- [x] A failed stream setup no longer leaves the runtime unusable: every
      attempt builds a fresh command queue.
- [x] `Cargo.toml` optimizes only the audio crates (`oxisynth*`, `lewton`,
      `soundfont`) in the dev profile: soundfont load went from ~56 s to
      ~8 s in a debug build here.
- [x] The runtime exposes `is_playing()`. It clears itself 2 s after the last
      scheduled event, so the UI never shows a finished song as playing.
- [x] Tests: loader errors for a missing and a corrupt soundfont, the play /
      stop / finish flag transitions, and an ignored test that loads the real
      soundfont and renders 2 s of non-silent audio.

### 2. `Space` plays and stops
- [x] `Space` toggles playback (from the start, like the Play button) and is
      ignored while a text field owns the keyboard. `Enter` still places the
      note.

### 3. Zoom and scrolling
- [x] **`Shift`+wheel zooms** the score around the pointer (since 19.11.2026;
      the plain wheel scrolls); `Ctrl`+wheel and pinch do too. It drives the same `pixels_per_tick` as the toolbar
      slider (shared `MIN_PIXELS_PER_TICK` / `MAX_PIXELS_PER_TICK`).
      Rows reflow to the page width at every zoom level, so only the row under
      the pointer is held at its screen height.
- [x] **The wheel scrolls the score vertically** (since 19.11.2026), and keyboard edits
      scroll the cursor's row into view. The 8-system cap is gone: rows are
      culled by the page rect instead.
- [x] Page placement lives in one `PageGeometry` (`page_geometry`) instead of
      five copies of the same offsets, so painting, click hit-testing, area
      selection, stretch edges and the cursor all see the scroll offset.

### 4. Edition palette redone in the Guitar Pro 8 / TuxGuitar style
- [x] Grouped 22 px vector icons (`src/icons.rs`, drawn with the painter so
      they follow the theme) in an 8-column grid with thin separators:
      bar, durations, dynamics, techniques, text/chords. Active duration,
      dynamic and effects are highlighted with the accent color.
- [x] Wired: time signature popup (shared with the context menu), add /
      insert / delete bar, the six durations, tie, the eight dynamics and all
      14 note effects. Commands that don't exist yet are drawn disabled with
      a "(planned)" tooltip.
- [x] Dynamics and effects now apply to every selected note (single or area
      selection); an effect button flips only that effect on the selection.
- [x] Palette and fretboard colors are `EditorTheme` fields for both themes.

### 5. Fretboard is interactive
- [x] Click a string/fret to write the note at the edit cursor (same path as
      the keyboard, cursor advances). `Shift`+click keeps the cursor on the
      beat to build chords; clicking a note that is already there removes it.
- [x] Geometry in `src/fretboard.rs` (unit-tested): equal-tempered fret
      spacing from the track's `fret_count`, a clickable open-string zone left
      of the nut, hit testing shared with painting. Strings have graded
      thickness, inlays and fret numbers; the cursor string is tinted and the
      notes sounding at the cursor are marked.
- [x] `View > Fretboard` hides the panel; its height follows the string count.

### 6. RMB context menu: Bar / Note / Effects didn't expand
- [x] Root cause: `submenu()` (18.08.2026, item 7) was built on egui's
      `MenuButton`, which in egui 0.34 is a standalone click-to-toggle popup:
      it never opens on hover and a click on it closed the whole menu. The
      unmodified repo behaves the same, so this predates the 12.11.2026
      changes. Inside a menu `submenu()` now uses `SubMenuButton`, the same
      rule as egui's own `Ui::menu_button`, keeping
      `PopupCloseBehavior::CloseOnClickOutside` for the controls inside.
- [x] Checked in an Xvfb session: hovering opens Bar, Note and Effects, the
      pointer can move into a submenu without it closing, the nested Time
      Signature submenu opens and stays open, and `Add Bar` from the Bar
      submenu runs ("Added a bar after bar 2.").

### 7. Dragging bricks shows the bricks, not a line
- [x] Moving and stretching bricks used to draw an orange / blue vertical
      line with a tick caption. The preview now shows the notes themselves:
      their old place is dimmed with a dashed outline, and the bricks as they
      will land are drawn translucent in the accent color with their fret
      numbers, across row breaks too. The caption reports the real resulting
      change (for example `+1920 ticks, string -2`, `No change` when blocked).
- [x] The preview runs the same edit as the drop on a scratch copy of the
      project (`NoteDragPlan` + `apply_note_drag`, shared by
      `finish_canvas_drag`), so clamping against neighbouring notes and bar
      limits appears in the preview exactly as it will on release.
- [x] Fixed: a drag was hit-tested and measured from where the pointer was
      when egui reported the drag start, a few points after the press. A quick
      flick could miss the 10 px edge grab zone, and every drop was offset by
      that distance. It now starts from `pointer.press_origin()`.
- [x] Tests: the plan only changes the notes it names, a scratch copy gives
      the same notes the real project ends up with (and leaves the project
      untouched), and a zero drag changes nothing.
- [ ] `Esc` clears the selection but does not cancel a drag in progress.
- [ ] The classic-notation staff above the tab does not preview a drag.

Checked in this pass: `cargo test` (90 passed, 1 ignored, which was run
separately), and a scripted Xvfb session for wheel zoom, a fretboard click,
`Shift`+click, click-to-delete, `Space` without an audio device (status bar
reports it, no crash), the View-menu toggle, the bright theme, the
right-click submenus and the move / stretch drag previews.

## 13.11.2026 ✅ Implemented / Fixed this pass

### 1. Playback starts at the selection or edit cursor
- [x] **Play and `Space` start where you are working**: from the first
      selected note (single or area selection), otherwise from the edit
      cursor. Clicking, arrowing and selecting a note all move the cursor, so
      the cursor alone covers those; a selection dragged away from the cursor
      still wins (`playback_start`). The status line reports the bar playback
      begins in.
- [x] **The audio clock starts at the start tick, not at zero.** The `Play`
      command already carried a `start_tick` that only skipped earlier events,
      so a mid-song start would have waited out the skipped time in silence.
      `locate_tick` maps the start tick through the tempo segments and the
      callback begins on that segment and sample offset, so tempo automation
      before the start point is honoured.
- [x] Nothing at or after the start point: Play reports it in the status bar
      and does not start an empty 2 s tail.
- [x] Tests: the clock offset (a later start sounds immediately), earlier
      events skipped, a start past the last event only waits for the release
      tail, `locate_tick` across tempo segments, playback continuing on the
      later tempo, and start-tick selection rules. `cargo test`: 97 passed, 1
      ignored (the real-soundfont render, not run in this pass). Not heard on
      a real audio device.

### 2. Per-track sound, volume and pan reach the synth
- [x] **Each track's sound, volume and pan drive its MIDI channel.** Play
      sends a `ChannelSetup` per track (bank 0 select, program change,
      controller 7, controller 10), and the old "every channel is program 24"
      loop in `build_synth` is gone. `sync_mix` runs once per frame and sends
      a `SetChannels` command only when the project's setup changed, so edits,
      undo, redo and project loads are heard immediately while playing.
- [x] **Mute and solo now take effect**, by silencing the channel's volume
      (a muted track, or every other track while one is soloed), so they also
      work mid-playback and unmuting restores the track's own volume.
- [x] **Track table is editable**: Solo / Mute toggles, a sound picker, a
      volume drag value (0 to 127) and a pan drag value (`L64` to `R63`, `C`
      centre; typing `L20`, `R5`, `C` or a plain number works). Changing the
      sound, solo or mute is one undo step; a drag on volume or pan is one
      step however long it is. The table scrolls and the panel is taller.
- [x] **Sounds per instrument type** (`core/sounds.rs`): the picker lists the
      sounds suited to the track's type (acoustic guitar, electric guitar,
      bass, other), keeps a current sound that is outside that list
      selectable, and the "All sounds" checkbox lists all 128 General MIDI
      sounds. Names are the bundled soundfont's own preset names.
- [x] **Channels never land on the drum channel.** The synth treats MIDI
      channel index 9 as the drum kit whatever the program, so `.tg` import
      assigns channels with `melodic_channel`, which skips it.
- [x] **`.tg` import reads each channel's volume and balance** (they were
      discarded and reset to 100 / centre), and types an imported track from
      its sound (bass programs become bass, clean to harmonics electric).
      Export shares the pan mapping, so volume and pan round-trip.
- [x] Tests: channel setups (sound, volume, pan, mute, solo), the setup
      events and their order, sync only sending real changes, sound lists and
      names, drum-channel skipping, pan mapping and parsing, `.tg` round trip
      of sound/volume/pan, and, in the real-soundfont test, that program 33
      on channel 0 loads "Fingered Bass". `cargo test -- --include-ignored`:
      113 passed. The track table was exercised in the running app under a
      virtual display (open the picker, "All sounds", mute, volume and pan
      drags, undo); no audio device was available to listen to the result.

### 3. Track panel shows every track, scrolls and resizes
- [x] **The track list fills the whole bottom panel.** Its content used a
      one-row-tall `horizontal` layout, so the list and the separator stopped
      mid-panel and only the first track was visible. The content now gets
      the panel's full height; the list scrolls with an always-visible
      scrollbar at the right, and the panel resizes by dragging its top edge
      (default 110 px, up to 420; the code allows down to 72, but in testing
      it would not shrink below about 110).
- [x] The table uses fixed-width columns (`TRACK_COLUMNS`), so the header
      row and the "All sounds" checkbox stay outside the scrolling list and
      line up with it. The checkbox sits above the Sound column.

### 4. Copy, cut and paste notes
- [x] **`Ctrl+C`, `Ctrl+X`, `Ctrl+V`** and Edit > Cut / Copy / Paste work on
      the selected note or the rubber-band selection (`core/clipboard.rs`).
      egui reports these as `Copy` / `Cut` / `Paste` events, not key presses;
      copying also puts a text on the system clipboard, because egui only
      reports a paste while it holds text, and our own copy is pasted only
      while that text is still there (what else happens to Ctrl+V is in
      19.11.2026 item 1).
- [x] **Paste goes to the edit cursor** and leaves exactly the new notes
      selected (including the tied pieces tab view cuts at barlines). Pasted
      notes replace what they land on, on the same string: a note starting
      inside a pasted note is removed and one sounding across its start is
      cut there. Notes on strings the track lacks are skipped, and a tie
      survives only when the note it continues was copied too.
- [x] **Past the last bar**: when the cursor is at or beyond the end of the
      bars, one bar is appended (it continues the last bar's time
      signature) and the notes go to its start. Bars are also appended so a
      long paste ends inside the track. Cut and paste are one undo step each.

### 5. Dotted and double-dotted notes
- [x] **Entry**: `.` toggles a dot and `Ctrl+.` two dots for the notes being
      written and for the selected note; the toolbar has `.` and `..`
      toggles next to the duration, the palette's dotted and double-dotted
      buttons are enabled, and Note > Duration lists them. The dots stay on
      until toggled off, like the duration itself.
- [x] **Detection from the brick length** (`core/length.rs`): a length that is
      exactly a note value with up to two dots (whole to 128th, plus dotted
      and double-dotted wholes) is recognized. The staff shows it with its
      dots, bricks show their dots at the end, and selecting a note says what
      it is ("dotted quarter note (1/4.)"). Any other length shows as the
      longest standard length that fits, capped at a whole note.
- [x] Notes shortened to fit before the next note or the barline now land on
      the longest standard length including dotted ones; stepping a note's
      length with the arrows keeps its dots.
- [x] **Tab view rhythm marks**: tab view draws a stem per beat under the
      strings, with flags or beams for short values and a dot for each
      augmentation dot (a whole note is an open head). The marks come from
      the same bar data as the staff, so they agree with it. They show with
      the classic staff too (16.11.2026 item 6).
- [x] **`.tg` dotted durations**: import read a double-dotted length as 1.25
      times the value (it is 1.75), and export wrote every dotted length as
      its plain value. Both are fixed and round-trip.
- [x] Tests: dotted lengths, detection and labels, notation for double dots,
      dotted wholes and every standard length, the longest-fitting rule,
      length descriptions, `.tg` round trip of dotted lengths, and the
      clipboard rules above. `cargo test -- --include-ignored`: 135 passed.
      Panel layout, resizing, copy/paste (including past the last bar),
      dotting by key and the tab rhythm marks were exercised in the running
      app under a virtual display. Audio was tried by the user with the
      per-track sounds from section 2.

## 15.11.2026 ✅ Implemented / Fixed this pass

### 1. Playback ends with the last bar that has notes
- [x] **The transport stops at the end of the last bar holding a note**, over
      every track (`BwxProject::playback_end_tick`, passed to the callback as
      `end_tick` in the `Play` command). Finishing only clears the playing
      flag; the synth is not told to stop, so releases, reverb and delay ring
      out. One click on Play restarts the song after it ended. Supersedes the
      fixed 2 s wait after the last event (13.11.2026 item 1 and the
      `is_playing` note in the audio section); `END_TAIL_SECONDS` is gone. An
      explicit Stop still silences everything.
- [x] Tests: the end tick (bar that a note ends in, note ending on a barline,
      no notes), the song finishing when the clock reaches the end tick, a
      start past the end finishing at once.

### 2. Typing in a value field no longer writes notes
- [x] While a text field has focus (Scale, Vol, Pan, velocity, name boxes)
      `handle_keyboard` ignores every key: digits, arrows, Enter, Delete,
      `L`, minus/plus, undo. Before, only Space was guarded, so a typed value
      also wrote a fret at the cursor.

### 3. Add Track
- [x] **`Add Track` button** under the velocity slider in the bottom panel
      (`BwxProject::add_track_like`): an empty track shaped like the selected
      one (instrument type, tuning, sound, bar count, time signatures), a
      unique name ("Acoustic Guitar 2"), the next free channel, unmuted. It
      becomes the selected track, the table scrolls to it, and it is one undo
      step.

### 4. Instrument column in the track table
- [x] **`Instrument` picker per track** (Acoustic Guitar, Electric Guitar, Bass,
      Other) between Name and Sound. It sets the clef and which sounds the
      Sound list offers (`InstrumentTrack::set_stringed_variant`). A sound
      that was one of the old type's own suggestions moves to the new type's
      default; a sound picked from outside that list is kept. Tuning and
      notes are never touched, so a guitar track switched to Bass keeps its
      six strings.

### 5. Brick and tab rows no longer overlap
- [x] `SystemMetrics::STRING_GAP` is 14 px and every brick is exactly that
      tall, so notes on neighboring strings touch instead of overlapping, in
      brick and tab view. The remaining hard-coded 9 px spots (hit tests,
      cursor cell, drag snapping) use the constant.

### 6. Rests can be deleted in tab view
- [x] **Click a rest, press Delete/Backspace** (`BwxProject::delete_rest`).
      The notes after it in the same bar move left by the rest's length and
      the bar's tail fills with rests again. A note that sustains past the
      barline or is tied to a note in the next bar stays put along with
      everything from it onward, so the freed space opens just before it. A
      rest with nothing to pull into it (trailing, or right before such a
      note) reports that and changes nothing.

### 7. Lengths that are no single note value are tied automatically
- [x] **Tab view**: `split_bricks_at_barlines` also cuts a note into standard
      lengths (`core/length.rs::standard_pieces`, longest first, a remainder
      under a 128th folds into the last piece). The slices are
      `TieOrigin::BrickSplit`, so brick view merges them back. A quarter
      stretched by a sixteenth is a quarter tied to a sixteenth.
- [x] **Staff, both views** (`notation.rs`): every note is written as its
      pieces (`note_slices`), each a head with `tie_from`; a brick crossing a
      barline continues as a tied head in the next bar. Supersedes "any other
      length shows as the longest standard length that fits" (13.11.2026
      item 5) for notes in a project; `note_value_for` remains for fixtures.

### 8. Brick view draws no rests under the strings
- [x] The dashes below the tab in brick view (end-of-bar rests) are gone.
      Supersedes "brick view keeps end-of-bar rests" (11.11.2026): the staff
      still shows rests for every gap in both views. `tail_padding_rests` and
      `RestFill::BarEnd` stay in core, marked `allow(dead_code)`, with tests.

### 9. Bricks share the cursor's rounded corners
- [x] A brick is drawn with the cursor box's corner radius. A fragment of a
      brick that continues through a barline or row break stays square and
      unstroked on that side, so the pieces still read as one block.

### 10. Last written fret replaces the default fret
- [x] The `default fret` toolbar field is gone. Enter with nothing typed
      writes the fret of the note written last (0 at start).

### 11. Clefs and rests drawn from vector outlines
- [x] The treble and bass clefs and the quarter to 64th rests are TuxGuitar's
      outlines (`src/symbols.rs`), rasterized once into coverage masks since
      egui cannot fill concave paths. The "8" stays under the clef. Whole and
      half rests are blocks sized like TuxGuitar's. Shorter than a 64th share
      the 64th rest. The source is LGPL-2.1 or later.

### 12. Moving playback line
- [x] While playing, a line moves along the score of the selected track by
      the tempo clock alone (`AudioRuntime::playback_tick`, fractional
      ticks), interpolated between beat columns in tab view. It keeps moving
      when another track is selected, and the view scrolls to the row being
      played when that row changes. It is drawn in the accent color, 2.5
      points wide, reaching 16 points past the staff and the tab.

### 13. Bar timeline replaces the shadow timeline strip
- [x] The diamond strip under the score is gone. The track table's right
      side shows one row of bar blocks per track on a shared time axis: a
      block is as wide as its bar lasts, so a 3/4 bar is three quarters of a
      4/4 one and polymeter lines up. The shortest bar never drops below 30
      points (the rest grow with it and the row scrolls sideways); a few
      bars stop growing at about 170 points for a 4/4 bar. A click selects
      the track and moves the cursor to the bar start. Blocks show the bar
      number, the signature where it changes, whether the bar has notes, the
      cursor's bar and the bar being played. The table columns are narrower
      to make room. The timeline's horizontal scrollbar sits at the bottom
      of the panel, not under the last row; the track columns beside it
      scroll vertically with it and share one offset.
- [ ] Missing: a context menu on the blocks (selection and dragging:
      16.11.2026 items 10 to 12).

### 14. Paste moves the cursor past what it pasted
- [x] After a paste the cursor lands on the first beat at or after the end
      of the last pasted note (`PasteOutcome::end_tick`), so pasting a
      quarter note moves it a quarter on, and a second paste goes right
      behind the first. The pasted notes stay selected. Supersedes the
      cursor staying on the paste start (13.11.2026 item 4).

### 15. Score scrollbar
- [x] A vertical scrollbar along the right edge of the score area (thumb
      drag, click to jump). It shows only when the score is taller than the
      page.

### 16. Cursor box has no placeholder mark
- [x] The "_" in the middle of the cursor box is gone; the box only shows a
      ghost fret while one is being typed.

- [x] Tests: `cargo test`: 168 passed, 1 ignored (the real-soundfont render,
      not run). Exercised in the running app under a virtual display: row
      heights, rounded bricks, the clef, rests, the bar timeline, its click and
      its scrollbar, the score scrollbar, paste moving the cursor,
      Add Track, the Instrument picker, deleting a note then its rest, and
      typing in the Scale box writing no note. The playback line was drawn
      from a fixed test position, since there is no audio device there; the
      clock position itself is covered by a unit test. Not exercised in the
      app: tie arcs on screen, deleting a rest next to a tied note, the stop
      at the end of the song.

## 16.11.2026 ✅ Implemented / Fixed this pass

### 1. Right-click keeps the selection
- [x] A right-click on empty space only moves the edit cursor there; a
      right-click on a note does nothing to the selection. Neither unselects
      anything, so the context menu acts on the selection you already made.

### 2. Shift+Up/Down moves a whole selection
- [x] With an area selection, Shift+Up/Down shifts every selected note by a
      semitone on its own string (`BwxProject::shift_note_frets`) instead of
      moving the fretboard's string focus. If any note would leave the neck,
      none moves and the status line says so. A single selected note works
      as before.

### 3. Fretboard shows what is playing
- [x] While the song plays, the neck marks the notes of the selected track
      that sound at the playback position (`fretboard::sounding_markers`): a
      filled marker for the first sixteenth of a note, a ring while it rings
      on, tied continuations as rings. The cursor's string band is hidden
      then; with playback stopped it shows the cursor's notes as before.

### 4. Themes
- [x] **View > Theme**: mode (System/Dark/Bright), the preset list and
      `Customize theme...`, a window with the preset picker and name, a color
      for the bright and the dark background, Accent 1, Accent 2, a Highlight
      (timeline bars being played), a Warning color, an optional background
      image with a visibility slider, and an Advanced section that sets the
      color of each part (selection and toggles, edit cursor, playback line,
      bar timeline, left sidebar, fretboard, selected notes, tempo
      automation) to Accent 1, Accent 2 or its own color.
- [x] **Accent 1 is the one accent everywhere by default**: the blue the
      track list used, now lightened to `#4ca4f0` so it stands next to the
      green (an old saved default `#005c80` is moved to it). The bar
      timeline, edit cursor, playback line, left sidebar, selected notes,
      fretboard, tempo automation and egui's own selection (track list, the Notation and similar toggles) all
      follow it. Accent 2 defaults to the old green (`#40c4b8`) and nothing
      uses it until an element is pointed at it. Supersedes the green accent
      of 15.11.2026 items 12 and 13.
- [x] Every theme has a bright and a dark variant; the background color of
      the active variant drives all surfaces, and text, lines and the
      fretboard wood follow how light that background really is
      (`EditorTheme::from_preset`). egui's widgets use the same palette
      (`EditorTheme::visuals`); the warning color is egui's warning text
      color and the color of a bar in the timeline that a drag would cut
      from a tied neighbour.
- [x] **Presets**: 8 built-in (BetterWriter, Nordic Night, Sunset Jazz, Forest
      Floor, Ocean Deep, Rose Quartz, Graphite and Gold, Midnight Violet).
      They cannot be changed or deleted; rename a copy and `Save as new`.
      Saved presets can be updated, reverted and deleted. The theme in use,
      the mode and the saved presets live in `betterwriter_theme.txt` next
      to the executable (plain text, `ThemeStore`).
- [x] **Background image** (PNG or JPEG, scaled to at most 2048 points a
      side): behind the score, which shows it through by the visibility
      setting, and behind the start screen.
- [ ] Missing: the image is not drawn behind the toolbars and panels.
      Accent 2 colors repeated bars in the timeline and Warning marks a bar
      whose tie a drag would break (16.11.2026 items 12 and 13).

### 5. Note menu works on a multi-selection; octave shift
- [x] The RMB Note and Effects entries (dynamics, effects, tie, duration,
      semitone shifts) are enabled for an area selection, not only for a
      single note.
- [x] **One Octave Up/Down** (`Alt+Shift+Up/Down`) shifts the selection by
      12 frets through the same path as the semitone shift, so the neck
      limit check applies. The menu shows the shortcuts that exist
      (`Shift+Up/Down` for a semitone).

### 6. Tab view shows note lengths like MuseScore
- [x] Rhythm marks under the strings: a longer stem, beams that keep their
      spacing and flags for short values, in tab view with or without the
      classic staff. The playback line runs down over them
      (`tab_rhythm_depth`) and the "P.M." label sits below them.

### 7. Plug-in scan at launch
- [x] `src/plugins/scan.rs` (was `vst_scan.rs`) looks for plug-ins in the
      standard folders of the platform on a background thread: VST3 (`.vst3`),
      VST2 (`.dll`/`.so`/`.vst`), CLAP, LV2 bundles and, on macOS, AU. A VST3
      bundle's classes, kind (instrument or effect) and developer come from
      its `moduleinfo.json` when it has one, otherwise from the plug-in's
      factory read in a short-lived child process (`betterwriter
      --probe-vst3 <bundle>`, 20 s limit), so a plug-in that crashes on load
      costs only that process. Other formats take the developer from the
      folder they sit in. The result is cached by path and modification time
      in `betterwriter_plugins.txt` and read back at the next start.
- [x] VST3 instruments are listed apart from VST3 effects ("VST3i: Name"
      against "VST3: Name"); the "?" kind label is gone.
- [ ] Missing: VST2, CLAP, LV2 and AU are only listed; they cannot be loaded
      into a chain yet (item 17).

### 8. Soundchain (FX)
- [x] An **FX** button at the left of the bottom status bar opens a window
      with the tracks table (name and chain summary) and, for the picked
      track, its plug-in chain: add from the scanned list, remove, move up
      and down, bypass. Rescan is a button there. The chain is saved in the
      `.bwx` file (`InstrumentTrack::sound_chain`) and edits are undoable.
- [x] Each track also has its own **FX** button in the tracks table, between
      M and Name. It selects that track and opens the Soundchain window on
      its chain (click again to close); the tooltip says how many plug-ins it
      holds. The **Global FX** pair (same window) sits in the bottom-left panel.
- [x] The FX button is a Reaper-style pair: **FX** (opens the chain) joined
      to a power button on its right that bypasses the whole chain
      (`InstrumentTrack::fx_bypassed`, saved in the `.bwx`, one undo step).
      Both stay plain until the track has plug-ins; then FX is filled with
      Accent 1 and the power icon is Accent 1; while bypassed both are
      **Warning 2**. The power button is inert while the chain is empty.
- [x] **Warning 2** is a new theme color (`ThemePreset::warning2`, a red,
      shown in the theme editor, saved in the theme file; files without it
      get the default). The mute button (M) is filled with it while a track
      is muted.
- [x] Superseded by item 17: the chain window, its contents and the hosting
      were rebuilt there.

### 9. Tracks can be renamed
- [x] Double-click a track's name, or right-click > Rename: Enter or
      leaving the field keeps the name, Escape cancels, an empty name keeps
      the old one. One undo step.

### 10. Selecting bars in the bar timeline
- [x] Click selects a bar, Ctrl+click adds or removes one, Shift+click
      selects a run on a track or, across tracks, every bar overlapping the
      time between the two. Selection is kept apart from the cursor, and
      clicking without a modifier still moves the cursor to the bar.
      Selections that no longer exist after an edit are dropped.

### 11. Bar differences on hover
- [x] With bars selected on several tracks, hovering one of them marks its
      start on every row and shows, on each other selected bar and in the
      tooltip, how far it starts ahead of or behind the hovered one and
      how much longer or shorter it is, in beats.

### 12. Dragging bars
- [x] Selected bars drag along their track and across tracks together, as
      one undo step (`BwxProject::move_bars` in `src/core/bars.rs`). Time
      signature and repeat signs travel with a bar; the target is marked
      while dragging and `Esc` cancels. A bar moved onto a track with fewer
      strings has its notes pulled to the lowest string.
- [x] A bar that would lose a tie to a neighbour it no longer sits next to
      shows in the Warning color while it is dragged (only that bar, also
      when several move). The drop is still allowed; the tie is cut at the
      bar edge and the status line says so.
- [ ] Missing: a bar dragged to the right of a track's last bar has no
      empty space to land in (it goes after the last bar).

### 13. Repeats
- [x] Repeat Open (`[`) and Repeat Close (`]`, played 2 to 8 times in the
      Bar menu) on the bar at the cursor (`BarMark`, saved with the track,
      kept on its bar by insert, delete and move). Close without an open
      repeats from the start or the previous close; nested regions replay
      the inner one on every pass. The score draws the signs and an "xN"
      label.
- [x] The bar timeline shows each extra play as a copy in Accent 2 after the
      region. Selecting a copy selects the bar it copies, so the original
      and all its copies highlight and move together. The copies exist in
      the timeline only, not in the score.
- [x] Playback runs through the repeats (`unrolled_for_playback`): the
      shadow timeline and the song length follow playback order, and the
      playback line in the score jumps back on every pass
      (`score_tick_at_playback`).
- [ ] Missing: alternate endings, Simile marks and a repeat count shown in
      the timeline copies.

### 14. Selected rows and toggles are softer, the rest stays see-through
- [x] Selected rows and toggled buttons (the track list, the Notation and
      similar toggles) were a solid accent, much brighter than the cursor and
      the timeline's selected bars, which are see-through tints of it. They
      are now a half mix of the selection color with the panel behind
      (`SELECTION_STRENGTH`), about the weight of a selected timeline bar,
      with text that follows `contrast_text`. The cursor, the timeline bars
      and the active sidebar tool keep their see-through accent fills and
      plain accent outlines.
- [x] The Highlight color only outlines the bar being played; it no longer
      fills it. Supersedes the filled playing bar of item 4.

### 15. Whole and half rests lose their extra line
- [x] Whole and half rests drew a thin line across the staff line they hang
      from or sit on, wider than the block itself, which read as a stray
      line above them. Only the block is drawn now (`paint_rest_symbol`).

### 16. Bottom panel tidy-up
- [x] The bottom-left panel is compact (about 110 px): track name, a short
      velocity slider ("Vel 108"), the **Global FX** pair and **+ Track**. The
      FX button left the status bar. Global FX looks like a track's FX pair:
      its button opens the Global FX chain (item 17) and its power button
      bypasses it. It is plain while that chain is empty, accent while it is in
      use, Warning 2 while bypassed.
- [x] The tracks table sizes its Name, Instrument and Sound columns to their
      longest entry (up to a cap), so every row lines up under the
      headings (`track_column_widths`).
- [x] Fret numbers in the fretboard markers use the proportional font, so a
      0 no longer shows the monospace face's dotted zero as a dark mark
      inside the circle.

### 17. Repeat buttons, FX Chain, plug-in hosting, scrolling (17.11.2026)
- [x] **Repeat Open / Repeat Close** in the side palette and the Bar menu are
      live. Close opens a small picker like the time-signature one: a drag
      value "Play N times" (2 and up, no upper list; `MAX_REPEAT_COUNT` 998),
      Apply and Remove. `BarMark::repeat_close` is a `u16`; older files read
      back unchanged.
- [x] One shared refresh rate: the window presents with `vsync: true` at the
      monitor's rate and nothing caps the bottom menu or the notation. The dev
      profile now optimises all dependencies (opt-level 3) and keeps the
      program's own code at 1, which is what held the UI below the refresh
      rate. Not measurable here (no monitor).
- [x] **Loops in the Block Timeline**: clicking, Ctrl+clicking or dragging a
      bar of a loop of more than one bar selects every bar of the loop
      (`InstrumentTrack::loop_bars`: the smallest multi-bar repeat region that
      holds the bar), and a drag moves the selected bars of a row as one block,
      keeping their order and gaps.
- [x] **One scroll speed**: the track table and the bar timeline are driven
      by a single wheel handler (`show_track_table`), so they cannot differ.
- [x] **FX Chain** (was "Soundchain"). Every FX button, one per track and
      **Global FX**, opens a window of its own for its own chain
      (`src/app/fx_chain.rs`); a chain starts empty and adding to one never
      touches another. Layout after Reaper: menu bar (FX: Add, Replace,
      Remove, Open FX window, bypass all; Options: Rescan), the chain on the
      left (checkbox = active, "VST3i: Name"), the picked plug-in on the right.
      Left click picks and shows it (replacing what the window showed), Remove
      is available once one is picked, right click opens the Reaper-style
      instance menu (open, bypass, rename, replace, move up/down, remove).
      A plug-in that is added opens at once; reopening a chain opens the one it
      showed last. The window fills its height, so it stretches from the bottom
      edge (checked under a virtual display).
- [x] **Add FX** is a window of its own with the installed plug-ins in a
      folder per developer, a search box (names and developers), Rescan,
      Add / Cancel, double click adds. The global chain is offered effects only.
- [x] **Global FX** is a chain on the whole mix (`master_chain`,
      `master_fx_bypassed` in the `.bwx`), independent of the tracks.
- [x] **Hosting** (`src/plugins/vst3.rs`, `host.rs`): VST3 bundles load
      through `libloading` and the `vst3` bindings: factory classes, component,
      controller (also a separate one), processing in 64-frame blocks, note
      events, parameter edits, saved state (stored in the slot and written on
      save). Each plug-in has a stable slot id; the host keeps instances while
      slots exist.
- [x] **Audio routing** (`src/audio/mod.rs`): the vendored `vendor/oxisynth`
      (one added method, `render_groups`) renders every MIDI channel to its
      own buffer. Track chain: soundfont, then the plug-ins in order, into the
      mix; Global chain on the mix. A **VST3 instrument** in a track's chain
      takes that track's notes and replaces the sound: the soundfont does not
      play the track at all (no leak, reverb send included; tested against the
      real soundfont), the track's mute/solo/volume/pan apply to the plug-in's
      output. Bypassed slots and chains drop out of the graph.
- [x] **Editor windows**: Windows opens the plug-in's own editor in a window of
      its own (`native_window.rs`, plain Win32 calls). Everywhere, the right
      half lists the plug-in's parameters with sliders as a fallback.
- [ ] Not hosted yet: VST2, CLAP, LV2, AU (listed, with a message when added);
      macOS loading; plug-in latency compensation, tempo/transport info for
      plug-ins, MIDI controllers other than notes, multi-output instruments,
      instruments in the global chain.
- [ ] **Not tested here**: any real-world plug-in (only the `vst3` crate's
      gain example and a small instrument built from it), the Windows editor
      window and the Windows loader (no Windows here; the Windows-only code
      is type-checked, not run), audio on a device.
- [x] Tests: `cargo test` 229 passed with the two test plug-ins built from the
      vst3 crate (`BW_TEST_GAIN_VST3`, `BW_TEST_SYNTH_VST3`, `BW_TEST_VST3`
      name the bundles; without them those tests pass quietly), 2 ignored (the
      soundfont tests, run by hand and passing).

- [x] Tests (before item 17): `cargo test`: 214 passed, 1 ignored (the real-soundfont render,
      not run). Exercised in the running app under a virtual display: the
      tab rhythm marks with the staff shown, repeat signs and the "x2" label,
      the repeat copy in the timeline, Ctrl+click selection across two
      tracks and the hover comparison, a bar drag with its target mark and
      Warning color, the drop with the tie cut and the status message,
      renaming a track, the FX window and the scan result (0 plug-ins: none
      installed there). Not exercised in the app: the multi-selection RMB
      menu and the octave shift, Shift+click, `Esc` during a drag, dragging
      across tracks, repeats during playback (no audio device; covered by
      unit tests), the scan on real plug-in bundles (unit-tested on made-up
      ones), the playback line over the rhythm marks.
      Item 14 and the theme items, from the parallel UI work: exercised under a virtual display: every
      theme item above in dark and bright, a user preset saved and written to
      the file, a background image loaded from the file, right-click keeping
      a selected note, Shift+Up on an area selection moving all four notes.
      The fretboard during playback and the cursor-less neck were drawn from
      a fixed test position (no audio device there). Not exercised in the
      app: the file picker for the image, deleting and reverting presets.

## 18.11.2026 ✅ Implemented / Fixed this pass

### 1. Releases (version 0.3.0)
- [x] `.github/VERSION` holds the version (0.3.0); `Cargo.toml` and
      `Cargo.lock` carry the same number. `.github/workflows/build.yml` and
      `.github/scripts/release.sh` work like the ones of XLM-Studio: a push to
      `main` with a new version builds Windows, Linux and macOS packages (the
      program, `assets/soundfonts` and the licence; `cargo test --locked`
      runs first) and creates release and tag `0.3.0`; the same version with
      changed code rebuilds the packages and moves the tag; docs-only changes
      only move the tag. `release.sh sync` writes the version into the two
      Cargo files. The soundfont is now looked up next to the program first
      (`default_soundfont_path`), because a packaged build has no source tree.

### 2. Playback survives a change of the sound card's rate or buffer
- [x] The stream's error callback and a once-a-second comparison of the
      default output's sample rate, channels and format tell the runtime that
      the device changed. It then rebuilds the stream, decodes the soundfont
      again for the new rate, sets the plug-ins up again at that rate
      (`Instance::set_sample_rate`, editors stay open) and, if a song was
      playing, carries on from where it was (`take_resume`). The idle window
      wakes once a second for the comparison.

### 3. Crackling with plug-ins
- [x] Causes removed in the host: the UI took the plug-in's lock every frame
      to reach its controller, so the audio thread skipped blocks (a click);
      `LivePlugin` now keeps the controller handle from the start. Every
      block allocated event lists and parameter queues; they are reused now
      (up to 64 parameters per block, the rest wait). A plug-in that flags its
      output as silent no longer leaves the previous block's samples in the
      output (buffers start cleared, flagged channels are zeroed). The audio
      thread flushes denormals to zero (x86_64 and aarch64), as DAWs do.

### 4. FX Chain window: selection, plug-in view (resizing: see 19.11.2026)
- [x] (Replaced on 19.11.2026: the window was drawn by `fx_chain.rs` with its
      own edge dragging; it is a window of the operating system now, which
      resizes and moves as every window does, so the drift no longer applies.)
- [x] The list takes several plug-ins: Ctrl+click toggles, Shift+click picks a
      range, Ctrl+A all. Delete or Backspace removes the picked ones while the
      pointer is over the window (taken before the score's own keys). Dragging
      reorders the picked ones together, with an insertion line; one undo step.
      Bypass and Remove from the menus work on the whole pick.
- [x] A **Plug-in UI / Parameters** switch sits at the right end of the FX
      Chain menu bar. Plug-in UI lays the plug-in's editor over the right half
      as a child window of the program's window (Windows); Parameters shows
      the sliders. Only one of them is shown, so the sliders no longer sit
      beside an open editor. A double click on a slider or its name puts the
      parameter back to the plug-in's default.
- [x] The instance menu has **Open in a separate window** (and Show in the FX
      Chain window); double click does the same. The window of its own has a
      working title bar, minimise and close, and a sizing frame and maximise
      button when the plug-in can be resized. Cause: the window used the
      system `STATIC` class, which answers every hit test as transparent, so
      nothing could be clicked; it now has a class of its own
      (`native_window.rs`).

### 5. Instruments and sounds as in MuseScore
- [x] The Instrument column opens a picker with MuseScore's 644 instruments
      (`src/core/instruments.rs`, from its `instruments.xml`): a genre list
      (Common, All instruments, ...), the families of the genre, and the
      instruments in MuseScore's order, with a search box that puts the
      earliest match first. Picking one sets the tab view type (guitar, bass,
      other), keeps the tuning and the notes, and moves an automatic sound.
- [x] The Sound column is MuseScore's menu for `MS Basic`: the current sound,
      then SoundFonts > MS Basic > Choose automatically and the categories
      (Pipe, Reed, Brass, Vocal, Piano, Organ, Guitar, Bass, Strings, World,
      Synth, Sound effects, Percussion, Drumsets), with a search box. Names
      come from the soundfont's own preset list; the Expr. variants stay out
      of the menu as in MuseScore.
- [x] **Choose automatically**: `InstrumentTrack::sound_auto`. The sound is
      the bank and program of the instrument's definition and follows the
      instrument; a sound picked by hand stays until automatic is chosen
      again. New tracks start automatic; files from before read back with the
      sound they had. Banks other than 0 now reach the synth (controllers 0
      and 32; bank 128 is the drum kits).
- [ ] No drum track type yet: a drum kit sound plays on a melodic channel.
      The 12 MuseScore families all list, but only plucked strings have a tab
      view and a fretboard.

### 6. Build and tests
- [x] `cargo check --all-targets` and `cargo test`: 255 passed, 2 ignored; the
      warning count is the same as before this pass. The tests that need a
      real VST3 bundle passed quietly (none built here).


## 19.11.2026 ✅ Implemented / Fixed this pass

### 1. Instrument list
- [x] The picker's list ran sideways (a vertical list inside a horizontal row);
      it is a scrolling vertical list now.
- [x] Notation-only twins are gone: the 14 `(tablature)` instruments and
      `Marimba (single staff)` are not listed and `(grand staff)` lost its
      suffix. BetterWriter chooses the view (tab, staff) apart from the
      instrument, so these changed nothing here. An instrument id from an
      older file (`...-tablature`, `...-single`) finds the instrument it was
      a twin of. 629 instruments remain. The clef variants (`treble clef`,
      `bass clef`, `C clef`) are still listed.

### 2. Floating windows leave the program's window
- [x] The FX Chain window, the Add FX browser, the instrument picker, the
      Theme window and the Automation Editor are windows of the operating
      system now (egui viewports, `floating.rs`): they can be dragged over
      the desktop and past the program's window. On Windows each is owned
      by the program's window (`SetWindowLongPtr(GWLP_HWNDPARENT)`, found
      by its title), so it stays above it and goes with it when it is
      minimised; plug-in windows of their own are owned the same way. Where
      the platform has no such windows egui shows them inside the program's
      window. The New Project dialog stays an egui window.
- [x] Move and resize are the system's, so the custom edge dragging of
      18.11.2026 (and its tests) is removed. Delete, Backspace and Ctrl+A in
      the FX Chain list and Delete in the Automation Editor are read from the
      window that has the keyboard.
- [ ] **Not tested here (Windows only)**: finding the window by title,
      the owner relation, and plug-in editors as children of the FX Chain's
      own window instead of the program's.

### 3. Plug-in editors freezing during playback
- [x] Probable cause: playback asked for a repaint after every frame, so the
      window's message queue was never empty and `WM_TIMER`, the lowest
      priority message, never came. Editors animated by timers (Neural DSP,
      Cradle) froze; ones that draw on their own thread (FabFilter) did not.
      Playback now asks for a frame every 16 ms.

### 4. Blank white editor area
- [x] Editors that were shown stay alive, hidden (up to 4 per chain, the one
      used longest ago closes), so switching between Plug-in UI and
      Parameters, or back to a plug-in, does not make the editor again.
- [x] Hiding an editor no longer resizes it to 1 x 1; a visible one is
      repainted at once (`RedrawWindow`) when it is shown.
- [x] While nothing plays, the editors of the other plug-ins of a chain are
      made in the background, one per frame, so picking one shows a finished
      editor. A VST3 editor has to be made on the thread that owns the
      window, so this is spread over frames, not moved to another thread.

### 5. Copy and paste across apps
- [x] Copying no longer leaves "BetterWriter: N notes (copy N)" on the
      clipboard. The system clipboard (`clipboard-rs`, `app/system_clipboard.rs`)
      now holds the notes as a MIDI file (`audio/midi`; `public.midi-audio` on
      macOS), as a `.mid` file in the temp folder offered as a file list, and
      as a plain-text tab (`NoteClip::ascii_tab`). The text is also what lets
      egui report Ctrl+V. Tied notes are merged into one MIDI note; the tempo
      is the one in force at the first copied note (`core/smf.rs`,
      `NoteClip::midi_notes`).
- [x] Paste: the editor's own copy is pasted while the clipboard text is
      still its own. Otherwise a MIDI file on the clipboard (as data or as a
      `.mid` file) is placed on the track: each note takes the free string
      that reaches its key at the lowest fret, and notes no string reaches are
      left out (`NoteClip::from_midi`). Edit > Paste is enabled for either.
- [x] Tests: MIDI file write/read (resolutions, running status, held notes,
      refusing bad files), tie merging, the text tab, string choice and a
      pasted MIDI clip.

### 6. Allow irrational time signatures
- [x] Bar RMB menu > Time Signature has an **Allow irrational** checkbox. On,
      the denominator is any whole number from 1 to 255 (4/6, 5/12); off, it
      steps through 1 to 32 again and snaps to the nearest of those. A bar
      that already has an irrational signature turns the box on.
- [x] A bar's length is `numerator * 3840 / denominator` rounded to the
      nearest tick (`TimeSignature::measure_ticks`, also the beaming unit in
      `notation::beat_ticks_for`). Powers of two are unchanged.
- [x] An irrational signature greys out the `.tg` / `.gp` export with its own
      warning (`CompatibilityReport::has_irrational_meter`).

### 7. TVP Automation
- [x] Points are grabbed at once: the graph reads raw pointer input (press
      position and movement) instead of egui's drag response, which only
      starts after a hold or several pixels of travel.
- [x] Points move in whole values (bpm, volume, pan step) and sixteenth-note
      positions; **Alt** moves them freely. The "snap to grid" toggle still
      turns the position snap off.
- [x] **Per-track automation** checkbox: each track gets its own tempo, volume
      and pan curves (`InstrumentTrack::{tempo,volume,pan}_points`). Turning
      it on copies the shared curves into tracks that have none, and the
      window names the track it edits. Off, the shared curves play again.
- [x] **Volume and Pan automation** (`core/automation.rs`): constant or
      progressive points, sent as MIDI CC 7 and CC 10 (a ramp is stepped).
      A muted track or one a solo silences keeps its volume at 0.
- [x] Playback is timed in seconds: every track runs through its own tempo
      curve (`core/tempo.rs`), so tracks at different bpm stay in step with
      the clock; the playback position is derived per track.

### 8. Bar timeline
- [x] The timeline axis is real time: a block's width is its length in
      seconds through its track's tempo curve, so a faster track gets
      narrower blocks, and the playing mark follows the clock.
- [x] Multi-selection hover: for bars at different bpm the offset and length
      difference are given in seconds with the tick difference
      (`+0 ticks`); the on-timeline offset label switches to seconds.
- [x] The hover also tells **when the bar lines meet again**: bars of each
      track counted from the compared bars ("in step again after 5 bars here,
      4 bars there"), marked when the meeting lies past the last written bar,
      or "never back in step". Computed with exact fractions
      (`tempo::resync`); a tempo ramp in either track makes it "cannot be
      worked out". Nothing is shown for bars that already start together
      with the same length.

### 9. Scrolling in notation
- [x] The wheel scrolls the score up and down; `Shift`+wheel (also `Ctrl`+wheel
      and pinch) zooms around the pointer. Help text updated.

### 10. Build and tests (items 5 to 9)
- [x] Merged with the 18.11.2026 and 19.11.2026 work above. `cargo test`: 284
      passed, 2 ignored. `cargo check --all-targets`: only the three warnings
      that were already there. Not exercised in a running app: grabbing and
      Alt-dragging points, the per-track checkbox, the timeline hover text, the
      checkbox in the RMB menu, wheel and Shift+wheel, and pasting into a real
      DAW or from Guitar Pro (the clipboard formats follow the clipboard
      crate's documentation, not checked against those programs). Playback
      resumed after a device change now continues at the interrupted time
      (seconds) on the selected track.

---

## 🚧 Not yet done / future work

### Current issues:
- [ ] "Choose automatically" for MS Basic soundfont does not choose palm muted guitar on palm muted notes, all per-note autochoosing requires verification
- [ ] Palm muting just says P. M. on an affected note instead of showing a dashed line like in MuseScore or Tuxguitar. Needs reimplementation.
- [ ] Note lengths aren't autoreadjusted for bars with irrational time signatures
- [ ] Theme switching (19.08.2026, item 2) covers the biggest-impact colors
      but not every hardcoded one in `app.rs` — smaller accents (e.g. the
      P.M. label, playhead, selection-rectangle colors) weren't audited
      individually. Worth a dedicated pass through the remaining
      `egui::Color32::from_rgb(...)` call sites if the Dark theme turns up
      any more low-contrast spots in practice.
- [ ] Note dragging (19.08.2026, item 1) can change string, but there's no
      equivalent for *stretching* a note's edges across strings (stretching
      only ever changes duration/start on the note's current string) — not
      requested, just noting the asymmetry. Dev note: that's called BENDING.
- [ ] Playback tempo ramps (`Progressive` transition) are approximated by a
      flat average-bpm segment for audio scheduling (tempo, per track or
      shared); the automation graph
      shows the true linear ramp, but sample-accurate ramped playback timing
      is still open. Worth a dedicated pass if precise tempo-ramp audio ever
      matters (e.g. a gradual accelerando).
- [ ] Ctrl+V with a clipboard that holds a MIDI file but no text (copied
      in another program) does nothing, because egui reports a paste only
      for text; Edit > Paste takes it. Which clipboard formats a given DAW
      or Guitar Pro 8 reads is unchecked.
- [ ] Irrational time signatures stop at denominators of 255 and are not
      exported to `.tg` / `.gp`; the numerator still stops at 32.
- [ ] `.tg` export writes a brick that spans a barline as one over-long
      beat; exporting from tab view (where it is already split into tied
      notes) is the faithful path. Unverified against TuxGuitar itself.

### Core notation engine (Phase 2)
- Right mouse click (RMB) in an area to open context menu with options:

- [ ] Cut (`Ctrl+X`)
- [ ] Copy (`Ctrl+C`)
- [ ] Paste (`Ctrl+V`)
- [ ] All-Track Cut (`Ctrl+Shift+X`)
- [ ] All-Track Copy (`Ctrl+Shift+C`)
- [ ] Special Paste... (`Ctrl+Shift+V`)
- Bar
    
    - [ ] Clef... (`K`)
    - [ ] Key Signature... (`Ctrl+K`)
    - [ ] Triplet Feel... (`Ctrl+/`)
    - [ ] Free Time (`|`)
    - [ ] Double Barline
    - [ ] Anacrusis (Pickup Bar)
    - [x] Repeat Open (`[`)
    - [ ] Alternate Endings...
    - [x] Repeat Close... (`]`)
    - [ ] Directions... (`D`)
    - [ ] Simile Marks
    - [ ] Multirest (`Ctrl+R`)
    - [ ] Force Break Line (`Ctrl+Return`)
    - [ ] Prevent Break Line
    - [ ] System Layout...
    
- Note
    
    - [ ] Insert a Beat (`Ins`)
    - [ ] Delete the Beats (`Shift+Del`)
    - [ ] Copy Beats at the End (`C`)
    - [ ] Staccatissimo
    - [ ] Tenuto
    - [ ] Tie Beat (`Shift+L`)
    - [ ] Rest (`R`)
    - [ ] Fermata... (`F`)
    - [ ] Accidentals
    - [x] One Octave Down (`Alt+Shift+Down`)
    - [x] One Octave Up (`Alt+Shift+Up`)
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
    
- Effects
    
    - [ ] Grace Note
    - [ ] Trill... (`N`)
    - [ ] Ornament
    - [ ] Tremolo
    - [ ] Sustain Pedal
    - [ ] Legato (`Shift+H`)
    - [ ] Left Hand Tapping (`(`)
    - [ ] Dead Slapped
    - [ ] Palm Mute on Beat (`Shift+P`)
    - [ ] Pick Scrape Out Downwards
    - [ ] Pick Scrape Out Upwards
    - [ ] Bend... (`B`)
    - [ ] Tremolo Bar... (`Shift+W`)
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
    - [ ] Fade Out (`>`)
    - [ ] Volume Swell (`Alt+<`)
    - [ ] Wah Open (`Ctrl+Alt+O`)
    - [ ] Wah Close (`Ctrl+Alt+C`)
    
- [ ] Left mouse drag to select area of notes, both vertically and horizontally, in tab view.
- [ ] **Tempo map** (`tempo_bpm` is project-global today; needs per-tick tempo
      changes for the shadow-timeline playback to be sample-accurate).
- [ ] Grace notes and tuplets.
- [ ] Rests are only split into plain note values (a dotted rest appears only
      when it is the length being written), and no length is split into tied
      dotted pieces when it is not a single standard value.
- [ ] Copy and paste carries notes only: no rests, bar properties or tempo,
      no paste into another track's bar grid beyond the cursor rule, and
      between two running copies of the app it goes through MIDI (strings
      and frets are chosen again, effects are lost).
- [ ] Real chord detection (multiple notes on one tick drawn as a chord block).
- [ ] Deleting and reordering tracks (`Add Track` exists).
- [ ] Bar adding/deleting
- [ ] Vertical bar placement to fill the screen efficiently
- [ ] Brick note length manual editing with pulling it left or right as in piano rolls.
- [ ] Virtual guitar neck/Fretboard panel: tuning changable from the panel.
      (Note input, show/hide and the open-string zone beside the nut shipped
      12.11.2026, item 5.)
- [ ] In timeline instead of diamonds for each note display a row of blocks with a block for each existing bar, a block with a width of a bar size/time measure (4/4, 3/4, 6/4) relative to all other bar sizes in the project, so 1/4 is 4x smaller than 4/4, but to make them all visible and clickable, the smallest cannot go smaller than some width amount of pixels, and if it gets to that, make everything get bigger. Each instrument has its own row of bar blocks.
- [ ] Starting menu with recently opened projects kept in a vertical list, double-clicking opens the project to the edit menu we currently have. There should be a button to create new project with options: 1: name ("Untitled" by default, if it already exists add 1 then 2, etc.), 2. First track instrument Currently only Stringed will work, replace it with an instruments menu. Stringed has options of Acoustic guitar, Electric guitar, Bass, Other. In the future each will have default playback sound that will be modified with VST3s. 3. Amount of strings selection (up to 12, but in the input menu make any value above inputable.) Add there a button for "Demo project" option will open what we currently have as open-on-startup. New projects are fully empty and only have 1 instrument and 1 bar at 4/4 set to default tempo of 120 bpm.
- [ ] Make instruments selectable to display multiple of their tabs/notations at the same time (current time of each instrument's notes is the key for synchronization here)

### Audio / playback (Phase 3)
- [ ] **Live playback playhead** rendered on the canvas synced to the audio
      callback (the engine plays, but there is no moving cursor yet).
- [ ] Transport scrubbing: rewind / fast-forward / jump-to-start actually move
      the playhead (buttons exist but are no-ops).
- [ ] Loop regions, metronome, count-in.
- [ ] Verify audible output on a real device: a headless test renders
      `MS Basic.sf3` to a non-silent buffer (12.11.2026), but the `cpal`
      stream has never been heard in this sandbox (no sound card).
- [ ] Playing from the middle does not sound notes already held at the start
      tick (a tied continuation chosen as the start point stays silent until
      its next attack).
- [ ] Fretboard clicks don't audition the note.
- [ ] Changing a track's sound, volume or pan does not audition: the new setup
      is heard the next time notes play (live while playing, immediately).
- [ ] Sounds other than the 128 General MIDI ones in bank 0 (the soundfont's
      variation banks and the 37 drum kits in bank 128) are not selectable,
      and there is no drum track type yet.
- [ ] Track table: instrument type (which sound list a track gets) can't be
      changed after creation, and channel assignment is not editable.
- [x] Real **VST3 hosting**: done in 17.11.2026 item 17 (the stub is gone).
- [ ] Hosting for VST2, CLAP, LV2 and AU, and VST3 loading on macOS.

### File formats (Phase 4)
- [ ] **`.tg` import/export** beyond the current TuxGuitar subset reader. Export can't be tested, blocked due to inability to delete test instrument with polythithms
- [ ] **`.gp` (Guitar Pro)** reader/writer (export path is a placeholder text
      bundle today).
- [ ] Multi-track bundle export wizard with a per-track checklist (currently
      exports all tracks unconditionally).

### UI / UX
- [ ] **Custom theme palette editor** (`EditorTheme` exists but is not
      user-editable at runtime).
- [ ] Score scrollbar widget (`Shift`+wheel is the only way to scroll) and a
      paged layout.
- [ ] Zoom holds only the row under the pointer in place; its horizontal
      position shifts because rows reflow to the page width.
- [ ] Edition palette commands still drawn disabled: key signature, clef,
      triplet feel, repeats, tuplet, fermata,
      crescendo / decrescendo, bend, harmonic, trill, grace note, tremolo
      bar, text, chord diagram. No voice 1/2 or select/draw modes.
- [ ] Note effect glyphs rendered on the staff (palm-mute text exists; others
      are modeled but not drawn).
- [ ] Drag-to-move notes; drag-select range.
- [ ] Notation limits: one voice per chord (the shortest note's value is
      used), no tuplets, grace notes, key signatures (every black key is a
      sharp) or clef changes; 5/8 and 7/8 beam by single eighths instead of
      3+2 groups.
- [ ] Tab view rhythm marks (shown only with the classic staff hidden) have
      no tuplets, no rest marks of their own (the tab rests stay as they
      were), and beam groups follow the staff's beat grouping only.
- [ ] Tab-view rests are derived, not stored: they re-chunk when the duration
      selection changes (e.g. three quarter rests become six eighth rests),
      unlike TuxGuitar where each rest beat keeps its own length.
- [ ] Tab view has no overlapping notes (a second voice): a note is always
      cut at the next beat that has notes.
- [ ] A single bar wider than the page isn't scrolled or clipped in tab view.
- [ ] Moving and stretching notes in tab view (disabled for now, see
      10.11.2026).


# Info for left menu:
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
    
