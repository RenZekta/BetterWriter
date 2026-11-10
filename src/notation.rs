//! Classic notation model for one bar: where each note sits on the staff, how
//! long it looks, which stems share a beam, and which accidentals to print.
//!
//! Everything here is geometry-free (staff steps and ticks, not pixels), so
//! the painter in `app.rs` only maps these results onto the canvas. The model
//! follows TuxGuitar: a note's staff position comes from its sounding MIDI
//! pitch, accidentals are remembered per bar, and stems point away from the
//! middle line.

use std::collections::HashMap;

use crate::core::project::{
    InstrumentTrack, RestNode, StringedVariant, TICKS_PER_QUARTER, TabNote, Tick,
};

/// Which clef a track is written in. Guitars use a treble clef and basses a
/// bass clef; both are written an octave above where they sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clef {
    Treble,
    Bass,
}

impl Clef {
    pub fn for_track(track: &InstrumentTrack) -> Self {
        if track.stringed_variant == StringedVariant::Bass {
            Clef::Bass
        } else {
            Clef::Treble
        }
    }

    /// Diatonic index (7 * octave + letter, C = 0) of the bottom staff line,
    /// in written pitch.
    fn bottom_line_index(self) -> i32 {
        match self {
            // E4
            Clef::Treble => 7 * 4 + 2,
            // G2
            Clef::Bass => 7 * 2 + 4,
        }
    }
}

/// Semitones added to the sounding pitch to get the written pitch.
const WRITTEN_OCTAVE_SHIFT: i32 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accidental {
    Sharp,
    Natural,
}

/// A staff position as diatonic steps above the bottom line: 0 is the bottom
/// line, 1 the first space, 8 the top line. Negative and >8 values need
/// ledger lines.
pub type StaffStep = i32;

/// Letter (0 = C .. 6 = B) and alteration (0 or +1 sharp) for each pitch
/// class, spelling every black key as a sharp.
const SPELLING: [(i32, i32); 12] = [
    (0, 0),
    (0, 1),
    (1, 0),
    (1, 1),
    (2, 0),
    (3, 0),
    (3, 1),
    (4, 0),
    (4, 1),
    (5, 0),
    (5, 1),
    (6, 0),
];

/// A pitch spelled as a diatonic index plus alteration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpelledPitch {
    /// 7 * octave + letter, C = 0 (so C4 = 28).
    pub diatonic: i32,
    pub alteration: i32,
}

pub fn spell(sounding_midi: i32) -> SpelledPitch {
    let written = sounding_midi + WRITTEN_OCTAVE_SHIFT;
    let octave = written.div_euclid(12) - 1;
    let (letter, alteration) = SPELLING[written.rem_euclid(12) as usize];
    SpelledPitch {
        diatonic: 7 * octave + letter,
        alteration,
    }
}

pub fn staff_step(clef: Clef, pitch: SpelledPitch) -> StaffStep {
    pitch.diatonic - clef.bottom_line_index()
}

/// How an onset is notated: note value (1 = whole, 2 = half, 4 = quarter,
/// ...) and number of augmentation dots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteValue {
    pub value: u32,
    pub dots: u8,
}

impl NoteValue {
    /// Number of flags (or beams) the value carries: 0 for quarter and
    /// longer, 1 for eighth, 2 for sixteenth, and so on.
    pub fn flags(self) -> u32 {
        if self.value >= 8 {
            (self.value / 4).ilog2()
        } else {
            0
        }
    }

    pub fn has_stem(self) -> bool {
        self.value >= 2
    }

    pub fn filled_head(self) -> bool {
        self.value >= 4
    }
}

/// The longest standard note value (plain or dotted) that fits in `ticks`.
/// Durations longer than a whole note show as a whole note, and ones below a
/// 128th as a 128th.
pub fn note_value_for(ticks: Tick) -> NoteValue {
    let q = TICKS_PER_QUARTER;
    let table: [(Tick, u32, u8); 14] = [
        (q * 4, 1, 0),
        (q * 3, 2, 1),
        (q * 2, 2, 0),
        (q * 3 / 2, 4, 1),
        (q, 4, 0),
        (q * 3 / 4, 8, 1),
        (q / 2, 8, 0),
        (q * 3 / 8, 16, 1),
        (q / 4, 16, 0),
        (q * 3 / 16, 32, 1),
        (q / 8, 32, 0),
        (q * 3 / 32, 64, 1),
        (q / 16, 64, 0),
        (q / 32, 128, 0),
    ];
    table
        .iter()
        .find(|(length, _, _)| *length <= ticks)
        .map(|&(_, value, dots)| NoteValue { value, dots })
        .unwrap_or(NoteValue {
            value: 128,
            dots: 0,
        })
}

/// One note head inside a chord.
#[derive(Clone, Copy, Debug)]
pub struct Head {
    pub note_id: u64,
    pub step: StaffStep,
    pub accidental: Option<Accidental>,
}

/// Notes sharing an onset, drawn as one stem. The look is taken from the
/// shortest note of the chord.
#[derive(Clone, Debug)]
pub struct Chord {
    pub tick: Tick,
    pub value: NoteValue,
    pub heads: Vec<Head>,
    pub stem_up: bool,
}

impl Chord {
    pub fn lowest_step(&self) -> StaffStep {
        self.heads.iter().map(|head| head.step).min().unwrap_or(0)
    }

    pub fn highest_step(&self) -> StaffStep {
        self.heads.iter().map(|head| head.step).max().unwrap_or(0)
    }
}

#[derive(Clone, Debug)]
pub enum ScoreItem {
    Chord(Chord),
    Rest { tick: Tick, value: NoteValue },
}

impl ScoreItem {
    pub fn tick(&self) -> Tick {
        match self {
            ScoreItem::Chord(chord) => chord.tick,
            ScoreItem::Rest { tick, .. } => *tick,
        }
    }
}

/// A run of consecutive chords joined by beams. `chords` are indices into
/// `BarScore::items`.
#[derive(Clone, Debug)]
pub struct BeamGroup {
    pub items: Vec<usize>,
    pub stem_up: bool,
}

#[derive(Clone, Debug, Default)]
pub struct BarScore {
    pub items: Vec<ScoreItem>,
    pub beams: Vec<BeamGroup>,
}

/// Step of the middle line; stems point down for notes on or above it.
const MIDDLE_STEP: f32 = 4.0;

/// Builds the notation for the bar `[bar_start, bar_end)` of `track`.
///
/// `rests` are the rests to draw in the bar (from
/// `BwxProject::bar_rests`); notes that sustain in from an earlier bar are
/// not repeated. `beat_ticks` is the beaming unit: chords are only joined
/// within one beat.
pub fn score_bar(
    track: &InstrumentTrack,
    clef: Clef,
    bar_start: Tick,
    bar_end: Tick,
    rests: &[RestNode],
    beat_ticks: Tick,
) -> BarScore {
    let mut onsets: Vec<&TabNote> = track
        .notes
        .iter()
        .filter(|note| note.abs_tick >= bar_start && note.abs_tick < bar_end)
        .collect();
    onsets.sort_by_key(|note| (note.abs_tick, note.string_index));

    // Accidentals are remembered for the rest of the bar, per staff line or
    // space, so a repeated C# only gets its sharp once.
    let mut remembered: HashMap<i32, i32> = HashMap::new();
    let mut items: Vec<ScoreItem> = Vec::new();

    let mut index = 0;
    while index < onsets.len() {
        let tick = onsets[index].abs_tick;
        let mut group: Vec<&TabNote> = Vec::new();
        while index < onsets.len() && onsets[index].abs_tick == tick {
            group.push(onsets[index]);
            index += 1;
        }
        let shortest = group
            .iter()
            .map(|note| note.duration_ticks)
            .min()
            .unwrap_or(TICKS_PER_QUARTER);
        let value = note_value_for(shortest);

        let heads: Vec<Head> = group
            .iter()
            .map(|note| {
                let pitch = spell(track.midi_key_for(note) as i32);
                let before = remembered.get(&pitch.diatonic).copied().unwrap_or(0);
                let accidental = if pitch.alteration != before {
                    remembered.insert(pitch.diatonic, pitch.alteration);
                    Some(if pitch.alteration > 0 {
                        Accidental::Sharp
                    } else {
                        Accidental::Natural
                    })
                } else {
                    None
                };
                Head {
                    note_id: note.id,
                    step: staff_step(clef, pitch),
                    accidental,
                }
            })
            .collect();

        items.push(ScoreItem::Chord(Chord {
            tick,
            value,
            heads,
            stem_up: true,
        }));
    }

    for rest in rests {
        items.push(ScoreItem::Rest {
            tick: rest.abs_tick,
            value: note_value_for(rest.duration_ticks),
        });
    }
    items.sort_by_key(ScoreItem::tick);

    let beams = group_beams(&mut items, bar_start, beat_ticks);

    // Chords that are not beamed choose their own stem direction.
    let beamed: Vec<usize> = beams.iter().flat_map(|group| group.items.clone()).collect();
    for (position, item) in items.iter_mut().enumerate() {
        if let ScoreItem::Chord(chord) = item
            && !beamed.contains(&position)
        {
            chord.stem_up = stem_points_up(&[&*chord]);
        }
    }

    BarScore { items, beams }
}

/// Joins consecutive flagged chords of the same beat into beam groups and
/// gives every chord in a group the group's stem direction.
fn group_beams(items: &mut [ScoreItem], bar_start: Tick, beat_ticks: Tick) -> Vec<BeamGroup> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut current_beat: Tick = -1;

    let flush = |current: &mut Vec<usize>, groups: &mut Vec<Vec<usize>>| {
        if current.len() >= 2 {
            groups.push(std::mem::take(current));
        } else {
            current.clear();
        }
    };

    for (position, item) in items.iter().enumerate() {
        let beat = (item.tick() - bar_start) / beat_ticks.max(1);
        match item {
            ScoreItem::Chord(chord) if chord.value.flags() > 0 => {
                if beat != current_beat {
                    flush(&mut current, &mut groups);
                    current_beat = beat;
                }
                current.push(position);
            }
            _ => flush(&mut current, &mut groups),
        }
    }
    flush(&mut current, &mut groups);

    groups
        .into_iter()
        .map(|positions| {
            let chords: Vec<&Chord> = positions
                .iter()
                .filter_map(|position| match &items[*position] {
                    ScoreItem::Chord(chord) => Some(chord),
                    ScoreItem::Rest { .. } => None,
                })
                .collect();
            let stem_up = stem_points_up(&chords);
            for position in &positions {
                if let ScoreItem::Chord(chord) = &mut items[*position] {
                    chord.stem_up = stem_up;
                }
            }
            BeamGroup {
                items: positions,
                stem_up,
            }
        })
        .collect()
}

/// Stems go up while the average head position is below the middle line.
fn stem_points_up(chords: &[&Chord]) -> bool {
    let mut sum = 0.0_f32;
    let mut count = 0.0_f32;
    for chord in chords {
        sum += (chord.lowest_step() + chord.highest_step()) as f32 * 0.5;
        count += 1.0;
    }
    count == 0.0 || sum / count < MIDDLE_STEP
}

/// Ticks per beaming unit for a time signature: the denominator's note, or
/// three of them in compound meters such as 6/8 and 12/8.
pub fn beat_ticks_for(numerator: u32, denominator: u32) -> Tick {
    let unit = TICKS_PER_QUARTER * 4 / denominator.max(1) as Tick;
    if denominator == 8 && matches!(numerator, 6 | 9 | 12 | 15) {
        unit * 3
    } else {
        unit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{InstrumentFamily, StringedVariant};

    fn guitar() -> InstrumentTrack {
        let project = crate::core::BwxProject::empty_with_track(
            "t",
            InstrumentFamily::Stringed,
            StringedVariant::AcousticGuitar,
            6,
        );
        project.tracks[0].clone()
    }

    fn note(id: u64, tick: Tick, duration: Tick, string: usize, fret: u8) -> TabNote {
        TabNote {
            id,
            abs_tick: tick,
            duration_ticks: duration,
            string_index: string,
            fret,
            velocity: 100,
            effects: Default::default(),
            tied_from_previous: None,
        }
    }

    #[test]
    fn guitar_open_high_e_is_written_in_the_top_space() {
        // Open high E sounds E4 (MIDI 64) and is written E5: the fourth
        // space, step 7 above the bottom line.
        let track = guitar();
        // String 0 is the highest-pitched string.
        let key = track.tuning[0] as i32;
        assert_eq!(key, 64);
        assert_eq!(staff_step(Clef::Treble, spell(key)), 7);
    }

    #[test]
    fn first_fret_high_e_sits_on_the_top_line() {
        assert_eq!(staff_step(Clef::Treble, spell(65)), 8);
    }

    #[test]
    fn open_low_e_needs_ledger_lines_below_the_staff() {
        // Sounds E2, written E3: seven steps below E4.
        assert_eq!(staff_step(Clef::Treble, spell(40)), -7);
    }

    #[test]
    fn black_keys_are_spelled_as_sharps_on_the_natural_below() {
        let c_sharp = spell(61 - 12);
        assert_eq!(c_sharp.alteration, 1);
        assert_eq!(c_sharp.diatonic, spell(60 - 12).diatonic);
    }

    #[test]
    fn bass_clef_puts_open_low_e_on_the_bass_staff() {
        // Bass E1 (28) is written E2, which is below G2's bottom line.
        let step = staff_step(Clef::Bass, spell(28));
        assert_eq!(step, -2);
    }

    #[test]
    fn clef_follows_the_lowest_string() {
        let mut track = guitar();
        assert_eq!(Clef::for_track(&track), Clef::Treble);
        track.stringed_variant = StringedVariant::Bass;
        track.tuning = vec![28, 33, 38, 43];
        assert_eq!(Clef::for_track(&track), Clef::Bass);
    }

    #[test]
    fn durations_map_to_note_values_and_dots() {
        let q = TICKS_PER_QUARTER;
        let value = |ticks| note_value_for(ticks);
        assert_eq!(value(q * 4), NoteValue { value: 1, dots: 0 });
        assert_eq!(value(q * 2), NoteValue { value: 2, dots: 0 });
        assert_eq!(value(q), NoteValue { value: 4, dots: 0 });
        assert_eq!(value(q / 2), NoteValue { value: 8, dots: 0 });
        assert_eq!(value(q / 4), NoteValue { value: 16, dots: 0 });
        assert_eq!(value(q * 3), NoteValue { value: 2, dots: 1 });
        assert_eq!(value(q * 3 / 2), NoteValue { value: 4, dots: 1 });
        // An odd length shows as the longest value that fits.
        assert_eq!(value(q + 100), NoteValue { value: 4, dots: 0 });
        // Longer than a whole note stays a whole note.
        assert_eq!(value(q * 8), NoteValue { value: 1, dots: 0 });
    }

    #[test]
    fn flags_and_heads_follow_the_value() {
        assert_eq!(NoteValue { value: 4, dots: 0 }.flags(), 0);
        assert_eq!(NoteValue { value: 8, dots: 0 }.flags(), 1);
        assert_eq!(NoteValue { value: 16, dots: 0 }.flags(), 2);
        assert_eq!(NoteValue { value: 32, dots: 0 }.flags(), 3);
        assert!(!NoteValue { value: 2, dots: 0 }.filled_head());
        assert!(NoteValue { value: 4, dots: 0 }.filled_head());
        assert!(!NoteValue { value: 1, dots: 0 }.has_stem());
    }

    #[test]
    fn eighth_notes_in_one_beat_are_beamed_together() {
        let mut track = guitar();
        let e = TICKS_PER_QUARTER / 2;
        track.notes = vec![
            note(1, 0, e, 5, 0),
            note(2, e, e, 5, 3),
            note(3, 2 * e, e, 5, 5),
            note(4, 3 * e, e, 5, 7),
        ];
        let score = score_bar(&track, Clef::Treble, 0, 3840, &[], TICKS_PER_QUARTER);
        let sizes: Vec<usize> = score.beams.iter().map(|group| group.items.len()).collect();
        assert_eq!(sizes, vec![2, 2]);
    }

    #[test]
    fn a_rest_breaks_a_beam_group() {
        let mut track = guitar();
        let e = TICKS_PER_QUARTER / 2;
        track.notes = vec![note(1, 0, e, 5, 0), note(2, e, e, 5, 3)];
        let rests = [RestNode {
            abs_tick: e,
            duration_ticks: e,
        }];
        track.notes.remove(1);
        let score = score_bar(&track, Clef::Treble, 0, 3840, &rests, TICKS_PER_QUARTER);
        assert!(score.beams.is_empty());
    }

    #[test]
    fn compound_meter_beams_three_eighths() {
        assert_eq!(beat_ticks_for(6, 8), TICKS_PER_QUARTER * 3 / 2);
        assert_eq!(beat_ticks_for(4, 4), TICKS_PER_QUARTER);
        assert_eq!(beat_ticks_for(5, 8), TICKS_PER_QUARTER / 2);
    }

    #[test]
    fn stems_point_away_from_the_middle_line() {
        let mut track = guitar();
        // A low note (string 5 is the low E) and a high one.
        track.notes = vec![
            note(1, 0, TICKS_PER_QUARTER, 5, 5),
            note(2, TICKS_PER_QUARTER, TICKS_PER_QUARTER, 0, 0),
        ];
        let score = score_bar(&track, Clef::Treble, 0, 3840, &[], TICKS_PER_QUARTER);
        let ups: Vec<bool> = score
            .items
            .iter()
            .filter_map(|item| match item {
                ScoreItem::Chord(chord) => Some(chord.stem_up),
                ScoreItem::Rest { .. } => None,
            })
            .collect();
        assert_eq!(ups, vec![true, false]);
    }

    #[test]
    fn a_repeated_sharp_is_only_marked_once_per_bar() {
        let mut track = guitar();
        // High e string, fret 2 = F#4 sounding.
        track.notes = vec![
            note(1, 0, TICKS_PER_QUARTER, 0, 2),
            note(2, TICKS_PER_QUARTER, TICKS_PER_QUARTER, 0, 2),
        ];
        let score = score_bar(&track, Clef::Treble, 0, 3840, &[], TICKS_PER_QUARTER);
        let marks: Vec<Option<Accidental>> = score
            .items
            .iter()
            .filter_map(|item| match item {
                ScoreItem::Chord(chord) => Some(chord.heads[0].accidental),
                ScoreItem::Rest { .. } => None,
            })
            .collect();
        assert_eq!(marks, vec![Some(Accidental::Sharp), None]);
    }

    #[test]
    fn a_natural_cancels_an_earlier_sharp_on_the_same_line() {
        let mut track = guitar();
        // F#4 then F4 sounding on the high e string (frets 2 and 1).
        track.notes = vec![
            note(1, 0, TICKS_PER_QUARTER, 0, 2),
            note(2, TICKS_PER_QUARTER, TICKS_PER_QUARTER, 0, 1),
        ];
        let score = score_bar(&track, Clef::Treble, 0, 3840, &[], TICKS_PER_QUARTER);
        let second = match &score.items[1] {
            ScoreItem::Chord(chord) => chord.heads[0].accidental,
            ScoreItem::Rest { .. } => None,
        };
        assert_eq!(second, Some(Accidental::Natural));
    }
}
