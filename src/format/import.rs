//! What the readers of foreign formats produce: a song laid out on one
//! shared bar grid, turned into a project here, plus a report of what the
//! file held that BetterWriter has no place for yet.

use std::collections::BTreeMap;

use crate::core::{
    BwxProject, NoteEffects, TICKS_PER_QUARTER, TempoPoint, Tick,
    project::{
        InstrumentFamily, InstrumentTrack, StringedVariant, TabNote, TempoTransition,
        TieOrigin, TimeSignature, TimeSignatureChange,
    },
    sounds::{melodic_channel, variant_for_program},
};

/// A loaded project and what did not come across.
pub struct Import {
    pub project: BwxProject,
    /// One line per kind of thing left out, e.g. "12 bends".
    pub report: Vec<String>,
}

/// Counts of what a file held that the project cannot keep.
#[derive(Default)]
pub struct Report {
    counts: BTreeMap<&'static str, usize>,
}

impl Report {
    pub fn skip(&mut self, what: &'static str) {
        self.skip_many(what, 1);
    }

    pub fn skip_many(&mut self, what: &'static str, count: usize) {
        if count > 0 {
            *self.counts.entry(what).or_default() += count;
        }
    }

    pub fn lines(&self) -> Vec<String> {
        self.counts
            .iter()
            .map(|(what, count)| {
                if *count > 1 {
                    format!("{count} x {what}")
                } else {
                    (*what).to_owned()
                }
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ImportedBar {
    pub signature: TimeSignature,
    pub repeat_open: bool,
    /// Extra plays of the region the bar closes; 0 when it closes none.
    pub repeat_close: u16,
}

#[derive(Clone, Debug)]
pub struct ImportedNote {
    pub start: Tick,
    pub duration: Tick,
    /// Index into the track's tuning, whose first entry is the highest string.
    pub string_index: usize,
    pub fret: u8,
    pub velocity: u8,
    pub effects: NoteEffects,
    /// Sounds on from the previous note of the same string.
    pub tied: bool,
}

#[derive(Clone, Debug)]
pub struct ImportedTrack {
    pub name: String,
    pub program: u8,
    pub bank: u16,
    pub volume: u8,
    pub pan: i8,
    /// Highest string first.
    pub tuning: Vec<u8>,
    pub fret_count: u8,
    pub solo: bool,
    pub mute: bool,
    /// Drums and other unpitched parts: kept as an empty track in their
    /// place, since the editor has no way to write them yet.
    pub percussion: bool,
    pub notes: Vec<ImportedNote>,
    /// Texts written above the score, with the tick they sit at.
    pub texts: Vec<(Tick, String)>,
}

impl ImportedTrack {
    pub fn new(name: impl Into<String>, program: u8) -> Self {
        let variant = variant_for_program(program);
        Self {
            name: name.into(),
            program,
            bank: 0,
            volume: 100,
            pan: 0,
            tuning: variant.default_tuning(variant.default_string_count()),
            fret_count: 24,
            solo: false,
            mute: false,
            percussion: false,
            notes: Vec::new(),
            texts: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct ImportedSong {
    pub title: String,
    pub bars: Vec<ImportedBar>,
    pub tempos: Vec<TempoPoint>,
    pub tracks: Vec<ImportedTrack>,
    pub report: Report,
}

/// The start of every bar of `bars`, and the end of the last.
pub fn bar_starts(bars: &[ImportedBar]) -> Vec<Tick> {
    let mut at = 0;
    let mut starts = Vec::with_capacity(bars.len() + 1);
    for bar in bars {
        starts.push(at);
        at += bar.signature.measure_ticks();
    }
    starts.push(at);
    starts
}

/// Appends a tempo point unless the tempo already holds at `tick`.
pub fn push_tempo(tempos: &mut Vec<TempoPoint>, tick: Tick, bpm: f32, glide: bool) {
    if let Some(last) = tempos.last_mut() {
        if last.at_tick == tick {
            last.bpm = bpm.clamp(1.0, 999.0);
            return;
        }
        if last.bpm == bpm && !glide {
            return;
        }
    }
    let mut point = TempoPoint::new(tick, bpm);
    if glide {
        point.transition = TempoTransition::Progressive;
    }
    tempos.push(point);
}

impl ImportedSong {
    pub fn into_import(mut self) -> Import {
        if self.bars.is_empty() {
            self.bars.push(ImportedBar {
                signature: TimeSignature::new(4, 4),
                repeat_open: false,
                repeat_close: 0,
            });
        }
        let starts = bar_starts(&self.bars);
        let end = *starts.last().unwrap_or(&0);

        let mut changes: Vec<TimeSignatureChange> = Vec::new();
        for (bar, &start) in self.bars.iter().zip(&starts) {
            if changes.last().is_none_or(|last| last.signature != bar.signature) {
                changes.push(TimeSignatureChange {
                    at_tick: start,
                    signature: bar.signature,
                });
            }
        }
        if self.tempos.is_empty() {
            self.tempos.push(TempoPoint::new(0, 120.0));
        }
        self.tempos.sort_by_key(|point| point.at_tick);

        let mut tracks = Vec::with_capacity(self.tracks.len().max(1));
        let mut next_note_id = 1;
        for (index, imported) in self.tracks.iter().enumerate() {
            let strings = imported.tuning.len().max(1);
            let variant = variant_for_program(imported.program);
            let mut track = InstrumentTrack {
                id: (index + 1) as u64,
                name: imported.name.clone(),
                channel: melodic_channel(index),
                program: imported.program.min(127),
                bank: imported.bank,
                sound_auto: false,
                instrument_id: String::new(),
                solo: imported.solo,
                mute: imported.mute,
                volume: imported.volume.min(127),
                pan: imported.pan,
                tuning: if imported.tuning.is_empty() {
                    StringedVariant::AcousticGuitar.default_tuning(6)
                } else {
                    imported.tuning.clone()
                },
                fret_count: imported.fret_count.clamp(1, 36),
                time_signature_changes: changes.clone(),
                notes: Vec::new(),
                bar_count: self.bars.len() as u32,
                instrument_family: InstrumentFamily::Stringed,
                stringed_variant: variant,
                bar_marks: Vec::new(),
                sound_chain: Vec::new(),
                fx_bypassed: false,
                tempo_points: Vec::new(),
                volume_points: Vec::new(),
                pan_points: Vec::new(),
                tuplets: Vec::new(),
                annotations: Vec::new(),
            };
            for (bar, &start) in self.bars.iter().zip(&starts) {
                if bar.repeat_open {
                    track.set_repeat_open(start, true);
                }
                if bar.repeat_close > 0 {
                    track.set_repeat_close(start, bar.repeat_close);
                }
            }
            for note in &imported.notes {
                if note.start < 0 || note.start >= end {
                    continue;
                }
                track.notes.push(TabNote {
                    id: next_note_id,
                    abs_tick: note.start,
                    duration_ticks: note.duration.max(1),
                    string_index: note.string_index.min(strings - 1),
                    fret: note.fret.min(track.fret_count),
                    velocity: note.velocity.clamp(1, 127),
                    effects: note.effects.clone(),
                    tied_from_previous: note.tied.then_some(TieOrigin::User),
                });
                next_note_id += 1;
            }
            track.sort_notes();
            for (tick, text) in &imported.texts {
                if (0..end).contains(tick) {
                    track.set_annotation(*tick, text);
                }
            }
            tracks.push(track);
        }
        if tracks.is_empty() {
            tracks.push(InstrumentTrack::standard_guitar(1, "Guitar", 0));
            let first = &mut tracks[0];
            first.time_signature_changes = changes;
            first.bar_count = self.bars.len() as u32;
        }

        let mut project = BwxProject {
            schema_version: 1,
            title: self.title,
            tempo_points: self.tempos,
            volume_points: Vec::new(),
            pan_points: Vec::new(),
            per_track_automation: false,
            tracks,
            next_note_id,
            master_chain: Vec::new(),
            master_fx_bypassed: false,
        };
        project.normalize_bar_counts();
        for (track, imported) in project.tracks.iter_mut().zip(&self.tracks) {
            if imported.percussion {
                // Notes of a drum kit would land on strings; keep the
                // track as a marker where the part was.
                track.bank = 128;
            }
        }
        Import {
            project,
            report: self.report.lines(),
        }
    }
}

/// Fits pitches onto the strings of one track: the lowest fret that gives
/// the key, on a string no sounding note holds. A key outside the strings'
/// range moves by octaves into it.
pub struct StringPlacer {
    tuning: Vec<u8>,
    fret_count: u8,
    /// Where the note sounding on each string ends, and its index.
    busy: Vec<Option<(Tick, usize)>>,
    /// Notes moved by octaves into range.
    pub shifted: usize,
}

impl StringPlacer {
    pub fn new(tuning: Vec<u8>, fret_count: u8) -> Self {
        let strings = tuning.len();
        Self {
            tuning,
            fret_count,
            busy: vec![None; strings],
            shifted: 0,
        }
    }

    /// Places `key` at `start` and returns `(string_index, fret)`. When every
    /// string that can play it is held, the note that holds the best one is
    /// cut short; its index in `notes` is returned for that.
    pub fn place(
        &mut self,
        key: u8,
        start: Tick,
        end: Tick,
        index: usize,
    ) -> (usize, u8, Option<usize>) {
        let lowest = self.tuning.iter().copied().min().unwrap_or(40);
        let highest = self
            .tuning
            .iter()
            .copied()
            .max()
            .unwrap_or(64)
            .saturating_add(self.fret_count);
        let mut key = i32::from(key);
        let mut moved = false;
        while key < i32::from(lowest) {
            key += 12;
            moved = true;
        }
        while key > i32::from(highest) {
            key -= 12;
            moved = true;
        }
        self.shifted += usize::from(moved);

        let fits = |string: usize| -> Option<u8> {
            let fret = key - i32::from(self.tuning[string]);
            (0..=i32::from(self.fret_count)).contains(&fret).then_some(fret as u8)
        };
        let mut best: Option<(bool, u8, usize)> = None;
        for string in 0..self.tuning.len() {
            let Some(fret) = fits(string) else { continue };
            let held = self.busy[string].is_some_and(|(until, _)| until > start);
            let candidate = (held, fret, string);
            if best.is_none_or(|current| candidate < current) {
                best = Some(candidate);
            }
        }
        let (held, fret, string) = best.unwrap_or((false, 0, self.tuning.len() - 1));
        let cut = if held { self.busy[string].map(|(_, at)| at) } else { None };
        self.busy[string] = Some((end, index));
        (string, fret, cut)
    }
}

/// A note of a file that may not say which string it is on: with a `key`
/// the string and fret are chosen here, without one the note's own are kept.
pub type Unplaced = (ImportedNote, Option<u8>);

/// Puts every note that has a key on a string of `tuning`, earliest first,
/// and returns all the notes in time order. Notes that start together on one
/// string are the same note. A note cut short to make room for the next on
/// its string keeps its place in the order.
pub fn place_notes(
    mut notes: Vec<Unplaced>,
    tuning: &[u8],
    fret_count: u8,
    report: &mut Report,
) -> Vec<ImportedNote> {
    notes.sort_by_key(|(note, _)| note.start);
    let mut placer = StringPlacer::new(tuning.to_vec(), fret_count);
    for index in 0..notes.len() {
        let (note, key) = &notes[index];
        let Some(key) = *key else { continue };
        let (start, end) = (note.start, note.start + note.duration);
        let (string_index, fret, cut) = placer.place(key, start, end, index);
        if let Some(at) = cut {
            let earlier = &mut notes[at].0;
            earlier.duration = (start - earlier.start).max(1);
        }
        let note = &mut notes[index].0;
        note.string_index = string_index;
        note.fret = fret;
    }
    let mut placed: Vec<ImportedNote> = notes.into_iter().map(|(note, _)| note).collect();
    placed.dedup_by(|b, a| a.start == b.start && a.string_index == b.string_index);
    report.skip_many(
        "notes outside the strings' range moved by octaves",
        placer.shifted,
    );
    placed
}

/// Fits `(start, duration, key, velocity)` notes onto the strings of `tuning`.
pub fn notes_on_strings(
    keys: &[(Tick, Tick, u8, u8)],
    tuning: &[u8],
    fret_count: u8,
    report: &mut Report,
) -> Vec<ImportedNote> {
    let notes = keys
        .iter()
        .map(|&(start, duration, key, velocity)| {
            (
                ImportedNote {
                    start,
                    duration,
                    string_index: 0,
                    fret: 0,
                    velocity,
                    effects: NoteEffects::default(),
                    tied: false,
                },
                Some(key),
            )
        })
        .collect();
    place_notes(notes, tuning, fret_count, report)
}

/// Names the quarter-note value `ticks` as a fraction of a quarter, for tuplets.
pub fn scaled(ticks: Tick, numerator: Tick, denominator: Tick) -> Tick {
    ((ticks * numerator + denominator / 2) / denominator.max(1)).max(1)
}

/// The length of a written note value: 1 whole, 2 half, 4 quarter...
pub fn value_ticks(value: u32, dots: u8) -> Tick {
    let base = TICKS_PER_QUARTER * 4 / Tick::from(value.max(1));
    let mut total = base;
    let mut part = base;
    for _ in 0..dots {
        part /= 2;
        total += part;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q: Tick = TICKS_PER_QUARTER;

    #[test]
    fn a_song_without_tracks_or_bars_still_makes_a_project() {
        let import = ImportedSong::default().into_import();
        assert_eq!(import.project.tracks.len(), 1);
        assert_eq!(import.project.tracks[0].bar_count, 1);
    }

    #[test]
    fn bars_repeats_and_signatures_reach_every_track() {
        let bar = |n, repeat_open, repeat_close| ImportedBar {
            signature: TimeSignature::new(n, 4),
            repeat_open,
            repeat_close,
        };
        let mut song = ImportedSong {
            bars: vec![bar(4, false, 0), bar(3, true, 0), bar(3, false, 2)],
            ..ImportedSong::default()
        };
        song.tracks.push(ImportedTrack::new("A", 25));
        song.tracks.push(ImportedTrack::new("B", 33));
        let project = song.into_import().project;
        for track in &project.tracks {
            assert_eq!(track.bar_count, 3);
            assert_eq!(track.time_signature_changes.len(), 2);
            let spans = track.bar_spans();
            assert_eq!(spans[1].start, Q * 4);
            assert!(track.bar_mark_at(spans[1].start).repeat_open);
            assert_eq!(track.bar_mark_at(spans[2].start).repeat_close, 2);
        }
        assert_ne!(project.tracks[0].channel, project.tracks[1].channel);
    }

    #[test]
    fn notes_past_the_last_bar_are_dropped_and_ties_kept() {
        let mut song = ImportedSong::default();
        let mut track = ImportedTrack::new("A", 25);
        let note = |start, tied| ImportedNote {
            start,
            duration: Q,
            string_index: 0,
            fret: 3,
            velocity: 90,
            effects: NoteEffects::default(),
            tied,
        };
        track.notes = vec![note(0, false), note(Q, true), note(Q * 40, false)];
        song.tracks.push(track);
        let track = &song.into_import().project.tracks[0];
        assert_eq!(track.notes.len(), 2);
        assert!(track.is_tied_continuation(&track.notes[1]));
    }

    #[test]
    fn the_report_counts_and_names_what_was_left_out() {
        let mut report = Report::default();
        report.skip("lyrics");
        report.skip_many("bends", 3);
        assert_eq!(report.lines(), ["3 x bends", "lyrics"]);
    }

    #[test]
    fn pitches_take_the_lowest_fret_on_a_free_string() {
        let tuning = StringedVariant::AcousticGuitar.default_tuning(6);
        let mut report = Report::default();
        // E4 is the open first string; E4 again while it rings goes to B3.
        let keys = [(0, Q * 2, 64, 90), (Q, Q, 64, 90), (Q * 2, Q, 40, 90)];
        let notes = notes_on_strings(&keys, &tuning, 24, &mut report);
        assert_eq!((notes[0].string_index, notes[0].fret), (0, 0));
        assert_eq!((notes[1].string_index, notes[1].fret), (1, 5));
        assert_eq!((notes[2].string_index, notes[2].fret), (5, 0));
    }

    #[test]
    fn a_key_below_the_strings_moves_up_by_octaves_and_is_counted() {
        let tuning = StringedVariant::AcousticGuitar.default_tuning(6);
        let mut report = Report::default();
        let notes = notes_on_strings(&[(0, Q, 28, 90)], &tuning, 24, &mut report);
        assert_eq!((notes[0].string_index, notes[0].fret), (5, 0));
        assert_eq!(report.lines().len(), 1);
    }

    #[test]
    fn a_note_on_a_held_string_cuts_the_one_ringing_when_no_string_is_left() {
        let tuning = vec![40];
        let mut report = Report::default();
        let notes = notes_on_strings(&[(0, Q * 4, 41, 90), (Q, Q, 42, 90)], &tuning, 24, &mut report);
        assert_eq!(notes[0].duration, Q);
        assert_eq!(notes[1].fret, 2);
    }

    #[test]
    fn note_values_and_tuplet_scaling() {
        assert_eq!(value_ticks(4, 0), Q);
        assert_eq!(value_ticks(4, 1), Q * 3 / 2);
        assert_eq!(value_ticks(8, 2), Q / 2 * 7 / 4);
        assert_eq!(scaled(Q / 2, 2, 3), 320);
    }
}
