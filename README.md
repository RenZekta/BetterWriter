# BetterWriter
This will be a tablature writer that's better than Guitar Pro, TuxGuitar and MuseScore. Powered by Rust, it's better by design.

Key features that others don't care to implement: 
1. Native polyrhythms support with unrestricted time signatures. All notes saved on a buffer for replay without bars. Visible bars in playback timeline are visibly scaled in width from a square based on a smallest time signature size.
2. Automatic rests ONLY at the end of bars. (instead of whenever a note gets shorter in MuseScore and TuxGuitar)
3. VST3 (and instruments) support. (Similar to MuseScore)
4. Custom color palettes, with access to color of each element.
5. Modern look.

Sound generation uses MuseScore's `MS Basic.sf3` soundfont (see `assets/soundfonts/`).

## How to run
You should have Rust installed

Use Run.bat

Or do the same manually:

cmd in the folder,
```
cargo run
```
Or download installer for your platform in Releases

## Contribution
Use the roadmap in [Plan.md](Plan.md) for general understanding of the project, make sure your agent follows [AGENTS.md](AGENTS.md)
