Role: You are an expert systems engineer specializing in high-performance audio software, digital audio workstations (DAWs), and the Rust ecosystem. Your task is to architect and build a modern, standalone tablature editor called "BetterWriter".

### 1. Workspace Context & Absolute Paths

The project workspace is organized locally. You have access to a freshly initialized Rust project and two massive open-source codebases to use as direct inspiration for notation layout and audio logic:

- Target Rust Project: "A:\a\1\BetterWriter" (Already contains a default `cargo init` template)

- Reference Notation App (Java): "A:\a\1\tuxguitar" (Analyze its logic for handling tablature input, fret calculations, and editing workflows)

- Reference Audio Engine (C++): "A:\a\1\MuseScore" (Analyze for advanced synth and notation-to-midi concepts)

- Targeted SoundFont Asset: "A:\a\1\MuseScore sound\MS Basic.sf3" (This file will be used for native playback)

### 2. Core Objective & Product Vision

BetterWriter is a completely independent, native Rust desktop application. It is NOT a fork. It aims to capture the fluid, efficient tablature notation editing style of TuxGuitar and combine it with the standalone VST3 hosting power of a DAW, while completely breaking away from legacy vertical grid restrictions to support native polyrhythms.

### 3. High-Level Architectural Stack

You will implement the application using the following modern Rust ecosystem crates:

- UI Layer: `egui` or `slint` (Optimized for low-latency custom graphics drawing)

- Audio Core: `cpal` (Low-latency audio I/O) and `wmidi` (MIDI message parsing)

- Synthesizer Engine: `oxisynth` or equivalent pure-Rust SoundFont renderer to natively play back the `MS Basic.sf3` asset.

- VST3 Hosting: `cutoff` to natively load third-party instrument and effect binaries into an internal audio signal chain.

- Serialization: `serde` and `rmp-serde` (MessagePack) or `bson` to manage our proprietary `.bwx` file format.

- Compatibility: `zip` for structural file archiving.

### 4. Ground Rules for Codebase Construction

- Clean Architecture: Separate the Core Music State Engine, the UI Render Layer, and the Audio Thread/Sequencer into decoupled modules. Communication between the UI and Audio threads must use lock-free, thread-safe channels or ring buffers to avoid audio glitching.

- No Legacy Code Sharing: Do not attempt to compile or link the Java or C++ source code. Use them strictly as a conceptual blueprint to understand how music notation algorithms translate from abstract theory to software data models, as well as for visuals.

### 5. Next Steps Instruction

I will provide the key points development plan detailing the Custom Theme Palette, the Fluid Tail-Padding Editor Mechanics, the Live Buffered Shadow-Timeline Player, and the Polymetric `.bwx` Compatibility Engine.

# Plan

I have located the `MS Basic.sf3` file from MuseScore in the "BetterWriter\assets\soundfonts\MS Basic.sf3", you can sidestep from building a custom synthesizer from scratch. You can run an internal instance of an open-source SoundFont synth engine inside your Rust binary to play back those exact samples natively.

Here is how your standalone Rust tablature editor—codenamed **`.bwx` Engine**—should be structured based on the libraries from your research.

## Architecture Flow Overview

```
       [ User Interface Layer (egui / Slint) ]
         Controls, Theme States, Custom Notation Grid
                             │
                             ▼
         [ Core Engine & State Machine (Rust) ]
          Manages .bwx data, shifts bars, maps time
                             │
            ┌────────────────┴────────────────┐
            ▼                                 ▼
   [ Audio Host (VST3) ]         [ SoundFont Synth (oxisynth) ]
   Loads VST3 Instrument/Amps      Processes MS Basic.sf3 samples
            │                                 │
            └────────────────┬────────────────┘
                             ▼
               [ Audio I/O Engine (cpal) ]
               Low-latency Stream to Speakers
```
## Phase 1: The UI and Custom Layout Grid

**Crates:** `egui` (Immediate-mode) or `slint` (Declarative UI)

The visual canvas needs to completely ignore standard vertical grid constraints to support your dynamic, proportional timeline.

- **Design Layout:** Build a canvas that maps timeline elements using a strict calculation rule based on an absolute `pixels_per_tick` multiplier.

- **The Custom Theme Engine:** Because you are handling drawing operations manually, create a central configuration struct that dictates theme rules. This makes it incredibly easy to color the notation text, staff lines, or canvas background on demand without messy inversion routines.

```Rust
struct EditorTheme {
    sheet_background: egui::Color32,
    notation_foreground: egui::Color32,
    staff_line_color: egui::Color32,
    accent_color: egui::Color32,
}
```


## Phase 2: Core Data Engine & Creative Tail-Padding

**Crates:** Standard library types (`Vec`, `HashMap`) or specialized serialization (`serde`, `serde_json`)

Your core music state tracks bars fluidly, snapping trailing rests strictly to the end of your measures.

- **The `.bwx` Format:** Store the project using a highly flexible JSON layout. Each instrument track contains its own separate `time_signature_changes` and note arrays mapped to clean, absolute tick timestamps.

- **Fluid Notation State:** When a user updates a note length in the middle of a measure, compute the timing delta. Loop through the subsequent notes in that track's measure block and shift their absolute tick properties backward to close the gap cleanly.

- **End-of-Bar Tail Pad:** Find the difference between the absolute tick end-point of your final note and the measure's full capacity limit. If space remains, dynamically synthesize standard musical rest nodes _only_ in that trailing gap.

## Phase 3: The Shadow-Timeline (Live Buffering Player)

**Crates:** `cpal`, `wmidi`, `oxisynth` or `libloading` (for C-bindings if processing compressed `.sf3` files)

### Your Core Concept

Bypass compile-on-play delays entirely. The engine should act like a professional DAW (such as Reaper). Every single time a user adds, moves, or edits a note on the visual interface, a background routine immediately pushes a hidden "shadow note" or timestamped MIDI event onto a running, globally cached linear event stream. When the play button is pressed, the playback engine instantly activates with zero lag.

### Implementation Overview

- **The Global Event Buffer:** Establish a thread-safe, lock-free memory ring buffer or synchronized vector (`Arc<RwLock<Vec<MidiPlaybackEvent>>>`) that stores the chronological, flattened playback data for the entire song.

- **Delta Compilation Strategy:** Write an internal event listener attached directly to your UI inputs. The exact microsecond a user inserts a note or shifts an index, a worker thread intercepts the change, clears only the modified section of the timeline, re-maps those absolute ticks, and silently commits the updated note array to the background master cache.

- **The Instant Playback Trigger:** When the user hits play, the high-priority audio callback thread does not calculate or look up any tracks. It simply reads the pre-loaded global event buffer sequentially from the active playhead position. As absolute timeline ticks hit a note marker, it triggers your synthesizer instance natively with absolute zero latency.

### Sound system

- **Reusing MuseScore Sounds:** Use a pure-Rust SoundFont rendering library like `oxisynth` (or Rust bindings for FluidSynth). At startup, point the engine to your extracted `MS Basic.sf3` file.

- **Synthesis Loop:** As the linear playhead reaches notes on the timeline, send standard note triggers (`wmidi`) to your internal SoundFont instance. The engine renders the raw audio samples from the MuseScore library file and feeds them straight into your `cpal` hardware output loop.

## Phase 4: Polymetric Architecture & Compatibility Upgrades

**Crates:** `zip` (Archive Bundler), `serde` / `rmp-serde` (For native `.bwx` serialization)

### Your Core Concept

Users must be granted format freedom of export options. Build the editor to run natively on your custom `.bwx` format.

Once polyrhythmic engine development is stable, introduce a smart backward-compatibility upgrade layer. If a score has a simple, standard timeline (no polyrhythms), allow the user to save natively to standard formats like `.tg` or `.gp`. If polyrhythms are active, gray out those specific export options with a context-aware warning tooltip. Provide an export checkbox configuration manager to bundle unaligned lines out into separate individual standard tab files inside a zipped archive automatically.

### Implementation Overview

- **Native Saving Architecture:** Build the primary file saving module strictly around your custom `.bwx` binary or text-based schema, writing out independent instrument tracks with local, unaligned `time_signature_changes`.

- **Context-Aware Formats Filter:** Design an internal structural validator that runs a safety scan on save actions. If the tracker confirms that bar boundaries desynchronize between any two tracks (confirming polyrhythms), flags are set to force standard format save buttons into a grayed-out/disabled state, displaying a hover hint: _"Polyrhythms detected, incompatible format."_

- **The Multi-Track Bundler Wizard:** Program an advanced export system attached to a UI checklist. If multi-track export is initiated, the engine loops through selected tracks, maps their positions down to standard single-timeline configurations, writes independent `.tg` or `.gp` file buffers, and writes them directly into an archive using the `zip` crate.
