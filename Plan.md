# BetterWriter — Development Plan

Living checklist for the BetterWriter tablature editor (`1\BetterWriter`).
Built on the architecture described in `BetterWriterPrompt.md`: a pure-Rust
egui front end, a tick-based core music engine, a SoundFont/audio pipeline, and
a polymetric `.bwx` format.

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
- [x] **Duration menu**: `-`/`+` cycle the active duration; `Ctrl+1`–`Ctrl+6`
      jump to a duration slot; toolbar `−`/`+` buttons and the combo box drive
      the same state.
- [x] `Esc` clears the buffer/selection; `Home` snaps the cursor to tick 0.
- [x] Mouse and keyboard workflows unified: clicking the staff now
      **repositions the cursor** (and selects the string) instead of silently
      inserting a fixed-fret note.
- [x] Fixed the lenghth of 1/32 notes for correct visualisation.

### Visual feedback (Phase 1 — rendering)
- [x] **Cursor cell** drawn on the tab staff: translucent accent box sized to
      the current duration, with the half-typed fret shown as ghost text.
- [x] Active duration surfaced in the transport toolbar (combo + `−`/`+`).
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

## 🚧 Not yet done / future work

### Core notation engine (Phase 2)
- [ ] **Per-track polymetric time signatures** in the editor UI (currently the
      model supports `time_signature_changes`, but there is no UI to edit them
      per track mid-score).
- [ ] **Tempo map** (`tempo_bpm` is project-global today; needs per-tick tempo
      changes for the shadow-timeline playback to be sample-accurate).
- [ ] Ties, grace notes, tuplets, dotted/double-dotted durations.
- [ ] Real chord detection (multiple notes on one tick drawn as a chord block).
- [ ] Undo / redo stack for edits.
- [ ] Instruments adding/deleting
- [ ] Bar adding/deleting
- [ ] Bar scrolling

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
      pass-through stub that only records loaded paths). Requires implementation of VST3 folder scanning (C:\Program Files\Common Files\VST3).

### File formats (Phase 4)
- [ ] **Native `.bwx` save/load round-trip** tested through the UI (serializer
      + tests exist, but no in-app "Open .bwx" smoke test against a real file).
- [ ] **`.tg` import/export** beyond the current TuxGuitar subset reader.
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
- [ ] Fretboard panel is display-only; clicking a fret should enter a note.
- [ ] Classic notation view above tablature should be toggleable on/off.
- [ ] Mode switching from brick painting to UI similar to Guitar Pro, TuxGuitar, etc. with fret numbers instead of bricks and intelligend rests filling at the end of bars.

### Project hygiene
- [ ] Resolve the 3 pre-existing clippy warnings (collapsible `if` in
      `apply_selected_duration`, clamp pattern in `paint_selected_track_page`,
      unit-value `let` in `format/tuxguitar.rs`).
