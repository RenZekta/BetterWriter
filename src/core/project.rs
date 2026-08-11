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
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BwxProject {
    pub schema_version: u16,
    pub title: String,
    pub tempo_bpm: f32,
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
            tempo_bpm: 120.0,
            tracks: vec![guitar, baritone],
            next_note_id: 8,
        }
    }

    pub fn track(&self, id: u64) -> Option<&InstrumentTrack> {
        self.tracks.iter().find(|track| track.id == id)
    }

    pub fn track_mut(&mut self, id: u64) -> Option<&mut InstrumentTrack> {
        self.tracks.iter_mut().find(|track| track.id == id)
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
