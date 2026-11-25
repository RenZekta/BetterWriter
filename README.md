# BetterWriter

A tablature writer that's better than Guitar Pro, TuxGuitar and MuseScore.
Written in Rust, better by design.

Key features that others don't care to implement:

- **Native polyrhythms.** Every track carries its own time signatures and
  tempo map. Notes live on an absolute tick buffer; bars are per-track, not
  a shared grid. Tempo, volume and pan are automated per track, and playback
  is timed in seconds, so tracks at different tempos stay in step.
- **Unrestricted meters.** Irrational denominators up to 255 (e.g. 4/6, 5/13).
  Note values, rests and beaming scale to the bar's denominator.
- **Polymeter bar timeline.** One row of bar blocks per track, each as wide
  as its bar lasts in seconds. Hovering selected bars shows the offset and
  when the bar lines meet again.
- **Unrestricted per-track repeats.** Each track keeps its own repeat signs
  and loops as nested regions, and allows tracks to repeat differently from each
  other. The bar timeline draws each extra pass as a copy and moves a loop
  as one block; in the unrolled view, editing one pass edits all of them.
- **Rests are derived, not stored.** Shortening a note never inserts rest
  blocks. Brick view keeps the tab rest-free; tab view and the staff fill
  gaps from the bar grid. No more calculating the rest length - only creativity.
- **VST3 hosting.** Per-track FX chains plus a global chain. A VST3
  instrument replaces the soundfont for its track; plug-in editors open in
  their own windows (Windows) and parameter changes are undoable.
- **Ghost tracks.** Overlay other tracks faintly on the one you edit,
  aligned in time through each track's own tempo map, with repeats unrolled.
- **Modern look, custom themes.** Eight presets, saved custom themes, a
  color for each element and an option for a background image.
- **Cross-app clipboard (used to be Guitar Pro only).** Copy puts a MIDI file and a text tab on the system clipboard; a MIDI file copied from another program pastes onto the
  track.
- **Format round trip.** Native `.bwx`, plus import and export of  `.gp`, `.tg`, `.mscz` and `.mid`. An export that cannot hold the score due to format limitations is disabled.

Sound generation uses MuseScore's `MS Basic.sf3` soundfont (see
`assets/soundfonts/`) and can use VST3i plugins.

## How to run

With a Rust toolchain installed, run `Run.bat`, or do the same manually:

```
cargo run
```

Installers for Windows, Linux and macOS are in Releases.

## Contribution

Read the roadmap in [Plan.md](Plan.md) for general understanding of the
project. Agents working on the code must follow [AGENTS.md](AGENTS.md).
