use serde::{Deserialize, Serialize};

pub type Tick = i64;
pub const TICKS_PER_QUARTER: Tick = 960;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl TimeSignature {
    pub const fn new(numerator: u8, denominator: u8) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    pub fn measure_ticks(self) -> Tick {
        let denominator = self.denominator.max(1) as Tick;
        self.numerator as Tick * TICKS_PER_QUARTER * 4 / denominator
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeSignatureChange {
    pub at_tick: Tick,
    pub signature: TimeSignature,
}

/// How a tempo point transitions into the *next* point on the automation
/// timeline. `Constant` holds the point's bpm flat until the next point (a
/// hard tempo change); `Progressive` ramps linearly toward the next point's
/// bpm, matching Guitar Pro's automation editor.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TempoTransition {
    Constant,
    Progressive,
}

/// One point on the project's tempo automation timeline. Tempo is global
/// (not per-track) since a piece has one clock even when individual
/// instruments run their own bar/time-signature grid.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct TempoPoint {
    pub at_tick: Tick,
    pub bpm: f32,
    pub transition: TempoTransition,
    /// Mirrors Guitar Pro's per-point "Hide automation" toggle: when set,
    /// the "♩=bpm" label isn't drawn above the staff at this point.
    pub hidden: bool,
}

impl TempoPoint {
    pub fn new(at_tick: Tick, bpm: f32) -> Self {
        Self {
            at_tick: at_tick.max(0),
            bpm: bpm.clamp(1.0, 999.0),
            transition: TempoTransition::Constant,
            hidden: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TabNote {
    pub id: u64,
    pub abs_tick: Tick,
    pub duration_ticks: Tick,
    pub string_index: usize,
    pub fret: u8,
    pub velocity: u8,
    #[serde(default)]
    pub effects: NoteEffects,
}

impl TabNote {
    pub fn end_tick(&self) -> Tick {
        self.abs_tick + self.duration_ticks
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteEffects {
    pub dead: bool,
    pub palm_mute: bool,
    pub let_ring: bool,
    pub vibrato: bool,
    pub slide: bool,
    pub hammer: bool,
    pub ghost: bool,
    pub accent: bool,
    pub heavy_accent: bool,
    pub staccato: bool,
    pub tapping: bool,
    pub slapping: bool,
    pub popping: bool,
    pub fade_in: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestNode {
    pub abs_tick: Tick,
    pub duration_ticks: Tick,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DurationChoice {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
}

impl DurationChoice {
    pub const ALL: [DurationChoice; 6] = [
        DurationChoice::Whole,
        DurationChoice::Half,
        DurationChoice::Quarter,
        DurationChoice::Eighth,
        DurationChoice::Sixteenth,
        DurationChoice::ThirtySecond,
    ];

    pub fn ticks(self) -> Tick {
        match self {
            DurationChoice::Whole => TICKS_PER_QUARTER * 4,
            DurationChoice::Half => TICKS_PER_QUARTER * 2,
            DurationChoice::Quarter => TICKS_PER_QUARTER,
            DurationChoice::Eighth => TICKS_PER_QUARTER / 2,
            DurationChoice::Sixteenth => TICKS_PER_QUARTER / 4,
            DurationChoice::ThirtySecond => TICKS_PER_QUARTER / 8,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DurationChoice::Whole => "1",
            DurationChoice::Half => "1/2",
            DurationChoice::Quarter => "1/4",
            DurationChoice::Eighth => "1/8",
            DurationChoice::Sixteenth => "1/16",
            DurationChoice::ThirtySecond => "1/32",
        }
    }
}

/// Broad category of a track's instrument. Only `Stringed` is fully wired up
/// today (tab entry, playback key mapping, fretboard panel); the rest exist
/// so the New Project picker reads correctly ahead of their implementation.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstrumentFamily {
    Stringed,
    Orchestra,
    Drums,
    Midi,
}

impl InstrumentFamily {
    pub const ALL: [InstrumentFamily; 4] = [
        InstrumentFamily::Stringed,
        InstrumentFamily::Orchestra,
        InstrumentFamily::Drums,
        InstrumentFamily::Midi,
    ];

    pub fn label(self) -> &'static str {
        match self {
            InstrumentFamily::Stringed => "Stringed",
            InstrumentFamily::Orchestra => "Orchestra",
            InstrumentFamily::Drums => "Drums",
            InstrumentFamily::Midi => "MIDI",
        }
    }

    /// Whether this family is actually playable/editable yet. Non-implemented
    /// families are still selectable in the picker (per the plan) but fall
    /// back to a Stringed track under the hood until they land.
    pub fn is_implemented(self) -> bool {
        matches!(self, InstrumentFamily::Stringed)
    }
}

/// Sub-type of a `Stringed` track. Only affects the default tuning/program
/// today; each variant is meant to carry its own default playback sound once
/// VST3 instruments land (see Plan.md).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum StringedVariant {
    AcousticGuitar,
    ElectricGuitar,
    Bass,
    Other,
}

impl StringedVariant {
    pub const ALL: [StringedVariant; 4] = [
        StringedVariant::AcousticGuitar,
        StringedVariant::ElectricGuitar,
        StringedVariant::Bass,
        StringedVariant::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StringedVariant::AcousticGuitar => "Acoustic Guitar",
            StringedVariant::ElectricGuitar => "Electric Guitar",
            StringedVariant::Bass => "Bass",
            StringedVariant::Other => "Other",
        }
    }

    pub fn default_string_count(self) -> usize {
        match self {
            StringedVariant::Bass => 4,
            _ => 6,
        }
    }

    /// General MIDI program used until VST3 patch loading replaces it.
    pub fn default_program(self) -> u8 {
        match self {
            StringedVariant::AcousticGuitar => 24,
            StringedVariant::ElectricGuitar => 27,
            StringedVariant::Bass => 33,
            StringedVariant::Other => 24,
        }
    }

    /// Builds a tuning of `count` strings, highest string first (matching
    /// this project's `tuning[0]` = highest-pitched-string convention).
    /// Counts at or below the instrument's standard string count keep the
    /// *highest* strings of the standard tuning (dropping the low end, the
    /// way guitarists usually think about reduced-string instruments).
    /// Counts above standard extend downward in perfect fourths, matching
    /// real extended-range instruments (7-string low B, 8-string low F#,
    /// 5-string bass low B, etc). Any count, however large, is accepted.
    pub fn default_tuning(self, count: usize) -> Vec<u8> {
        let count = count.max(1);
        let (base_low, steps): (i32, &[i32]) = match self {
            StringedVariant::Bass => (28, &[5, 5, 5]), // E1 A1 D2 G2
            _ => (40, &[5, 5, 5, 4, 5]),                // E2 A2 D3 G3 B3 E4
        };
        let standard_len = steps.len() + 1;

        let mut low_to_high = vec![base_low];
        for &step in steps {
            low_to_high.push(low_to_high.last().copied().unwrap_or(base_low) + step);
        }

        let low_to_high = if count <= standard_len {
            low_to_high.split_off(standard_len - count)
        } else {
            let mut extra_low = Vec::new();
            let mut pitch = base_low;
            for _ in 0..(count - standard_len) {
                pitch -= 5;
                extra_low.push(pitch.clamp(0, 127));
            }
            extra_low.reverse();
            extra_low.extend(low_to_high);
            extra_low
        };

        low_to_high
            .into_iter()
            .rev()
            .map(|pitch| pitch.clamp(0, 127) as u8)
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InstrumentTrack {
    pub id: u64,
    pub name: String,
    pub channel: u8,
    pub program: u8,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub mute: bool,
    #[serde(default = "default_track_volume")]
    pub volume: u8,
    #[serde(default = "default_track_pan")]
    pub pan: i8,
    pub tuning: Vec<u8>,
    pub fret_count: u8,
    pub time_signature_changes: Vec<TimeSignatureChange>,
    pub notes: Vec<TabNote>,
    /// Explicit number of bars this track has. Bars beyond content used to be
    /// implied purely by an ever-expanding render horizon; they're now a real
    /// part of the project so save/load round-trips a project's actual shape
    /// and so Insert/Add/Delete Bar have something concrete to operate on.
    pub bar_count: u32,
    pub instrument_family: InstrumentFamily,
    pub stringed_variant: StringedVariant,
}

impl InstrumentTrack {
    pub fn standard_guitar(id: u64, name: impl Into<String>, channel: u8) -> Self {
        Self {
            id,
            name: name.into(),
            channel,
            program: 24,
            solo: false,
            mute: false,
            volume: 100,
            pan: 0,
            tuning: vec![64, 59, 55, 50, 45, 40],
            fret_count: 24,
            time_signature_changes: vec![TimeSignatureChange {
                at_tick: 0,
                signature: TimeSignature::new(4, 4),
            }],
            notes: Vec::new(),
            bar_count: 4,
            instrument_family: InstrumentFamily::Stringed,
            stringed_variant: StringedVariant::AcousticGuitar,
        }
    }

    /// General track constructor used by the New Project dialog. Currently
    /// only `Stringed` tracks are implemented; other families still build a
    /// stringed-style tuning underneath so the track stays usable.
    pub fn new_track(
        id: u64,
        name: impl Into<String>,
        channel: u8,
        family: InstrumentFamily,
        stringed_variant: StringedVariant,
        string_count: usize,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            channel,
            program: stringed_variant.default_program(),
            solo: false,
            mute: false,
            volume: 100,
            pan: 0,
            tuning: stringed_variant.default_tuning(string_count.max(1)),
            fret_count: 24,
            time_signature_changes: vec![TimeSignatureChange {
                at_tick: 0,
                signature: TimeSignature::new(4, 4),
            }],
            notes: Vec::new(),
            bar_count: 1,
            instrument_family: family,
            stringed_variant,
        }
    }

    pub fn signature_at(&self, tick: Tick) -> TimeSignature {
        self.time_signature_changes
            .iter()
            .take_while(|change| change.at_tick <= tick)
            .last()
            .map(|change| change.signature)
            .unwrap_or(TimeSignature::new(4, 4))
    }

    pub fn measure_bounds_at(&self, tick: Tick) -> (Tick, Tick) {
        // A signature change starts a fresh bar grid at its own tick. This is
        // important for polymetric tracks: using the active signature to
        // divide from tick zero would put bars on the wrong side of a later
        // signature change. A change that falls mid-bar deliberately closes
        // that bar early and begins a new one.
        let tick = tick.max(0);
        let current_change_index = self
            .time_signature_changes
            .iter()
            .rposition(|change| change.at_tick <= tick);
        let (segment_start, signature, next_change_tick) = match current_change_index {
            Some(index) => (
                self.time_signature_changes[index].at_tick.max(0),
                self.time_signature_changes[index].signature,
                self.time_signature_changes
                    .get(index + 1)
                    .map(|change| change.at_tick),
            ),
            None => (
                0,
                TimeSignature::new(4, 4),
                self.time_signature_changes
                    .first()
                    .map(|change| change.at_tick),
            ),
        };
        let measure_ticks = signature.measure_ticks().max(TICKS_PER_QUARTER / 4);
        let start =
            segment_start + (tick - segment_start).div_euclid(measure_ticks) * measure_ticks;
        let nominal_end = start + measure_ticks;
        let end = next_change_tick
            .filter(|change_tick| *change_tick > start && *change_tick < nominal_end)
            .unwrap_or(nominal_end);
        (start, end)
    }

    pub fn midi_key_for(&self, note: &TabNote) -> u8 {
        self.tuning
            .get(note.string_index)
            .copied()
            .unwrap_or(40)
            .saturating_add(note.fret)
            .min(127)
    }

    pub fn sort_notes(&mut self) {
        self.notes.sort_by_key(|note| {
            (
                note.abs_tick,
                note.string_index,
                note.fret,
                note.duration_ticks,
                note.id,
            )
        });
    }

    /// End tick of the track's explicit bar count, walking `bar_count` bars
    /// forward from tick 0 through the (possibly polymetric) signature
    /// timeline. This is the real, save/load-stable extent of the track —
    /// distinct from wherever the notes happen to currently reach.
    pub fn bars_end_tick(&self) -> Tick {
        let mut cursor = 0;
        for _ in 0..self.bar_count.max(1) {
            let (_, end) = self.measure_bounds_at(cursor);
            cursor = end;
        }
        cursor
    }

    /// 0-based index of the bar containing `tick`, clamped to the last bar
    /// that actually exists (`bar_count - 1`).
    pub fn bar_index_at(&self, tick: Tick) -> u32 {
        let tick = tick.max(0);
        let mut cursor = 0;
        let mut index = 0u32;
        loop {
            let (_, end) = self.measure_bounds_at(cursor);
            if tick < end || index + 1 >= self.bar_count.max(1) {
                return index;
            }
            cursor = end;
            index += 1;
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BwxProject {
    pub schema_version: u16,
    pub title: String,
    /// Tempo automation timeline, replacing what used to be a single global
    /// `tempo_bpm`. Always has at least one point; the point at or before
    /// tick 0 governs playback until the next point takes over. See
    /// `tempo_at` / `interpolated_tempo_at`.
    pub tempo_points: Vec<TempoPoint>,
    pub tracks: Vec<InstrumentTrack>,
    pub next_note_id: u64,
}

impl BwxProject {
    pub fn demo() -> Self {
        let mut guitar = InstrumentTrack::standard_guitar(1, "Guitar", 0);
        guitar.notes = vec![
            TabNote {
                id: 1,
                abs_tick: 0,
                duration_ticks: TICKS_PER_QUARTER,
                string_index: 5,
                fret: 0,
                velocity: 110,
                effects: NoteEffects::default(),
            },
            TabNote {
                id: 2,
                abs_tick: TICKS_PER_QUARTER,
                duration_ticks: TICKS_PER_QUARTER / 2,
                string_index: 4,
                fret: 2,
                velocity: 108,
                effects: NoteEffects::default(),
            },
            TabNote {
                id: 3,
                abs_tick: TICKS_PER_QUARTER + TICKS_PER_QUARTER / 2,
                duration_ticks: TICKS_PER_QUARTER / 2,
                string_index: 3,
                fret: 2,
                velocity: 108,
                effects: NoteEffects::default(),
            },
            TabNote {
                id: 4,
                abs_tick: TICKS_PER_QUARTER * 2,
                duration_ticks: TICKS_PER_QUARTER,
                string_index: 2,
                fret: 1,
                velocity: 108,
                effects: NoteEffects::default(),
            },
        ];

        let mut baritone = InstrumentTrack::standard_guitar(2, "Polymeter sketch", 1);
        baritone.time_signature_changes = vec![TimeSignatureChange {
            at_tick: 0,
            signature: TimeSignature::new(5, 8),
        }];
        baritone.notes = vec![
            TabNote {
                id: 5,
                abs_tick: 0,
                duration_ticks: TICKS_PER_QUARTER / 2,
                string_index: 5,
                fret: 7,
                velocity: 94,
                effects: NoteEffects::default(),
            },
            TabNote {
                id: 6,
                abs_tick: TICKS_PER_QUARTER / 2,
                duration_ticks: TICKS_PER_QUARTER / 2,
                string_index: 4,
                fret: 5,
                velocity: 94,
                effects: NoteEffects::default(),
            },
            TabNote {
                id: 7,
                abs_tick: TICKS_PER_QUARTER,
                duration_ticks: TICKS_PER_QUARTER / 2,
                string_index: 3,
                fret: 4,
                velocity: 94,
                effects: NoteEffects::default(),
            },
        ];

        Self {
            schema_version: 1,
            title: "BetterWriter sketch".to_owned(),
            tempo_points: vec![TempoPoint::new(0, 120.0)],
            tracks: vec![guitar, baritone],
            next_note_id: 8,
        }
    }

    /// A brand-new, otherwise-empty project: one track, one 4/4 bar at 120
    /// bpm, no notes. Used by the New Project dialog.
    pub fn empty_with_track(
        title: impl Into<String>,
        family: InstrumentFamily,
        stringed_variant: StringedVariant,
        string_count: usize,
    ) -> Self {
        let track_name = if matches!(family, InstrumentFamily::Stringed) {
            stringed_variant.label().to_owned()
        } else {
            family.label().to_owned()
        };
        let track =
            InstrumentTrack::new_track(1, track_name, 0, family, stringed_variant, string_count);
        Self {
            schema_version: 1,
            title: title.into(),
            tempo_points: vec![TempoPoint::new(0, 120.0)],
            tracks: vec![track],
            next_note_id: 1,
        }
    }

    /// Bumps every track's `bar_count` up (never down) so it covers whatever
    /// notes are actually present. Used as a safety net after importing a
    /// foreign format (`.tg`): those files derive `bar_count` from their own
    /// measure headers, which should already cover every note, but a
    /// slightly malformed import shouldn't leave notes hanging past the last
    /// bar. Native `.bwx` saves always write `bar_count` themselves, so this
    /// isn't needed there.
    pub fn normalize_bar_counts(&mut self) {
        for track in &mut self.tracks {
            track.bar_count = track.bar_count.max(1);
            let Some(last_note_end) = track.notes.iter().map(TabNote::end_tick).max() else {
                continue;
            };
            // `bars_end_tick` is monotonically non-decreasing in `bar_count`,
            // so this always terminates.
            while track.bars_end_tick() < last_note_end {
                track.bar_count += 1;
            }
        }
    }

    pub fn track(&self, id: u64) -> Option<&InstrumentTrack> {
        self.tracks.iter().find(|track| track.id == id)
    }

    pub fn track_mut(&mut self, id: u64) -> Option<&mut InstrumentTrack> {
        self.tracks.iter_mut().find(|track| track.id == id)
    }

    /// The flat bpm governing `tick`: the latest tempo point at or before it.
    /// Falls back to 120 if the project somehow has no points at all (every
    /// constructor seeds one at tick 0, so this is only a defensive floor).
    pub fn tempo_at(&self, tick: Tick) -> f32 {
        self.tempo_points
            .iter()
            .filter(|point| point.at_tick <= tick)
            .max_by_key(|point| point.at_tick)
            .map(|point| point.bpm)
            .unwrap_or(120.0)
    }

    /// Like `tempo_at`, but linearly ramps through a `Progressive` point
    /// toward the next one instead of holding flat — this is what the
    /// automation graph and the on-staff "♩=bpm" labels use so they read as
    /// a smooth ramp, matching the Guitar Pro automation editor.
    pub fn interpolated_tempo_at(&self, tick: Tick) -> f32 {
        let mut sorted: Vec<&TempoPoint> = self.tempo_points.iter().collect();
        sorted.sort_by_key(|point| point.at_tick);
        let Some(governing_index) = sorted.iter().rposition(|point| point.at_tick <= tick) else {
            return sorted.first().map(|point| point.bpm).unwrap_or(120.0);
        };
        let governing = sorted[governing_index];
        if governing.transition == TempoTransition::Progressive
            && let Some(next) = sorted.get(governing_index + 1)
        {
            let span = (next.at_tick - governing.at_tick).max(1) as f32;
            let t = ((tick - governing.at_tick) as f32 / span).clamp(0.0, 1.0);
            governing.bpm + (next.bpm - governing.bpm) * t
        } else {
            governing.bpm
        }
    }

    pub fn allocate_note_id(&mut self) -> u64 {
        let id = self.next_note_id;
        self.next_note_id += 1;
        id
    }
}

fn default_track_volume() -> u8 {
    100
}

fn default_track_pan() -> i8 {
    0
}
