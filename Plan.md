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

## 🚧 Not yet done / future work

### Current issues:
- [ ]

### Core notation engine (Phase 2)
- [x] Right mouse click (RMB) in an area to open context menu with options:

- [ ] Cut (`Ctrl+X`)
- [ ] Copy (`Ctrl+C`)
- [ ] Paste (`Ctrl+V`)
- [ ] All-Track Cut (`Ctrl+Shift+X`)
- [ ] All-Track Copy (`Ctrl+Shift+C`)
- [ ] Special Paste... (`Ctrl+Shift+V`)
- [x] Bar
    
    - [ ] Insert Bar (to the left of selected bar) (`Ctrl+Ins` and `Ctrl+Shift+B`)
    - [ ] Add a Bar (to the right of selected bar) (`Ctrl+B`)
    - [ ] Delete Bar (`Ctrl+Del`)
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
    
