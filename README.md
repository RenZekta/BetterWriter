# BetterWriter
This will be a tablature writer that's better than Guiatar Pro, TuxGuitar and MuseScore. Powered by Rust, it's better by design.

Key features that others don't care to implement: 
1. Native polyrhythms support with unrestricted time signatures. All notes saved on a buffer for replay without bars. Visible bars in playback timeline are visibly scaled in width from a square based on a smallest time signature size.
2. Automatic rests ONLY at the end of bars. (instead of whenever a note gets shorter in MuseScore and TuxGuitar)
3. VST3 (and instruments) support. (Similar to MuseScore)
4. Custom color palletes, with access to color of each element.
5. Modern look.

RenZekta's note: I don't have a programming degree and I am not a professional programmer. This is only my hobby. I am a musician. I vibecoded it in Codex after days of unsuccessful attempts of building MuseScore and TuxGuitar in attempt to simply make a fork of them with personal desired modifications. Sadly, even with sterile code without any modifications, I wasn't able to do it because of old messy structure and issues with dependencies and their visibility. That's when I looked at Rust with its robust libraries/crates handling.

I plan to use MuseScore's `MS Basic.sf3` for sound generation.

## How to run

cmd in the folder,
```
cargo run
```
(you should have Rust installed)

## Contribution
If you want to contribute to the project, you probably know about programming more than I do, or have more tokens in vibecoding apps. So thank you for anything that works and good luck.
I include my prompt as BetterWriterPrompt.md that I used originally for you to clearly understand my goals with this project.
