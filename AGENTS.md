# Instructions for agents working on BetterWriter

This is a solo-developer hobby project. There's no PR review process and no
maintainer capacity to protect, so most of a typical AI-contribution policy
doesn't apply here. The one thing that matters: code and comments should read
as if a single careful engineer wrote them, not as a transcript of a chat
session.

## Comments

Keep comments concise. A comment should tell a future reader something the
code itself doesn't already say: a non-obvious invariant, a constraint
imposed by an external system (egui's layout rules, the SoundFont/MIDI
pipeline, TuxGuitar's file format), or the reason behind a choice that isn't
the obvious one. If the code is self-explanatory, don't add a comment just to
narrate it.

Do not reference the conversation that produced the change. No "Task 3",
"Item 2", "Bug fix (this round)", "per the user's request", "as discussed
above". None of that means anything to someone reading the file later. Write
the comment as if it's always been there.

If a comment is explaining a bug fix, describe the invariant or failure mode
itself, not the fact that a bug existed and was fixed. "X must happen before
Y because Z" is useful forever. "Fixed a bug where X happened before Y" stops
being useful the moment the surrounding code changes again.

```
// GOOD (explains a non-obvious constraint)
// Tab view keeps every note inside one bar, so anything that now crosses a
// barline after an edit is cut into tied notes here.

// BAD (narrates the fix, references "the user")
// Bug fix: previously a note could cross a barline in tab view. Now we split
// it after every edit as requested.
```

```
// GOOD (code is self-explanatory, no comment needed)
let end = note.abs_tick + note.duration_ticks;

// BAD (restates what the code already says)
// The end is the start tick plus the duration
let end = note.abs_tick + note.duration_ticks;
```

When editing existing code, don't leave the old comment in place alongside a
new one explaining the same thing differently. Replace it. A function should
have one coherent explanation of what it does, not a changelog of every pass
someone made over it. Doc comments that describe a feature as "a stand-in
until X exists" must be updated or removed when X lands.

## Text and writing

- Do not write the developer's personal information anywhere.
- Do not write descriptions as if explaining them to the developer (e.g.
  full-detail descriptions in README of every feature or change added during
  a pass).
- The comment rules apply to all writing: strings, menus, tooltips, status
  messages.
- Avoid obvious and self-explanatory descriptions.
- Write concisely. Nobody wants to read gigantic blocks of text.

## Plan.md

`Plan.md` is the roadmap and change log. Update it on every change, keep its
existing formatting (dated sections, `- [x]` items wrapped with a 6-space
continuation indent), and move a goal into the implemented area once it is
fully finished. Describe what now exists and what is still missing, not the
chat that led there. Don't record verification claims you didn't actually run.

## Code style

- Match the surrounding code's conventions rather than introducing a new
  pattern for the same problem.
- Prefer extending an existing helper over adding a parallel one that does
  almost the same thing.
- No em dashes, curly quotes, or other non-ASCII punctuation in code or
  comments: plain ASCII only. Musical glyphs (`♩` in a "♩=bpm" tempo label)
  are notation, not punctuation, and stay as they are.
- Keep music logic out of the painter. Tick-based rules (bars, ties, rests,
  notation spelling) live in `src/core/` and `src/notation.rs`, geometry-free
  and unit-tested; `src/app.rs` only maps their results onto the canvas.

## Before large changes

Two seams in this codebase are easy to break by changing only one side:

- **View modes.** Brick view and tab view store the same music differently
  (a sustained note is one brick vs. tied notes cut at barlines). Any new
  edit path must leave notes valid for the active mode, which is what
  `refresh_after_edit` and `normalize_for_view_mode` are for. Only
  `TieOrigin::BrickSplit` ties are re-merged when returning to brick view;
  `TieOrigin::User` ties must survive the round trip.
- **Canvas geometry.** Painting, click hit-testing, area selection, the edit
  cursor and tempo labels must all go through `layout_staff_systems`,
  `StaffSystem::x_of` / `tick_at`, `content_x` and `system_metrics`. Never
  compute an x position straight from `pixels_per_tick` for something drawn in
  tab view, and never hard-code staff offsets.

Trace the full path (data model, normalization, layout, painting, input)
before changing one piece of either.

## Building and checking

The project needs a Rust toolchain recent enough for `eframe` (1.92 or newer
at the time of writing) and ALSA headers on Linux.

```
cargo check --all-targets
cargo test
```

- Don't commit or zip `target/`.
- `vendor/oxisynth` is oxisynth 0.1.0 plus one method (`render_groups`, so each
  MIDI channel can be rendered apart for the plug-in chains). Keep it patched
  through `[patch.crates-io]`; don't edit anything else in it.
- Plug-in hosting lives in `src/plugins/` (`scan.rs`, `vst3.rs`, `host.rs`,
  `native_window.rs`). Tests that need a real VST3 run when `BW_TEST_VST3` /
  `BW_TEST_GAIN_VST3` / `BW_TEST_SYNTH_VST3` name a bundle (build the `gain`
  example of the `vst3` crate; the synth is a small variant of it) and pass
  quietly without. The Windows-only window code can be type-checked on other
  systems with `rustc --cfg bw_check` (see `native_window.rs`).
- When the deliverable is a zip of the repo, leave out `target/`, `.git` and
  `assets/soundfonts/MS Basic.sf3`. The soundfont never changes and is most
  of the zip's size; whoever unpacks over an existing copy keeps theirs. Put
  any other large asset (another soundfont, say) behind a download link or
  fetch it at runtime instead of shipping it in the zip.
- Don't run `cargo fmt` over the whole tree: the repo isn't fully
  rustfmt-clean, so it produces a large unrelated diff. Format only the code
  you touched.
- Newer compilers print an `f32` literal fallback warning for some existing
  `Stroke::new(1.0, ...)` calls. Use an explicit `_f32` suffix in new code so
  the warning count doesn't grow.

### Limited sandboxes

A cold build compiles egui, eframe and the audio crates, which takes several
minutes on one core and can exceed a sandbox's per-command time limit. A
command killed by the limit also kills the build it started.

- Run long builds detached and poll the log, so the limit only applies to the
  short polling commands:

  ```
  setsid nohup bash -c 'cargo test > /tmp/test.log 2>&1; echo EXIT:$? >> /tmp/test.log' \
      > /dev/null 2>&1 < /dev/null &
  tail -n 3 /tmp/test.log
  ```

- Run `cargo check --all-targets` while iterating: it skips code generation
  and is much faster, but it does not warm the build `cargo test` needs, so
  the first `cargo test` still compiles every dependency once.
- Compiled dependencies survive in `target/`, so after the first build,
  incremental `check` and `test` runs take seconds to a couple of minutes.
  Don't `cargo clean`, and don't change `Cargo.toml` or `Cargo.lock` without
  a reason, since that recompiles the dependencies.
- `cargo test <filter>` takes a single filter; use `-- --skip <name>` or
  separate runs for more.
- Skip the ignored real-soundfont test unless audio code changed.
