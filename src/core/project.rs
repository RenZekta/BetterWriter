use serde::{Deserialize, Serialize};

use super::automation::AutomationPoint;

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

    /// Denominators that are a power of two. Any other denominator is
    /// "irrational": its bar length is rounded to a whole tick and standard
    /// score formats cannot store it.
    pub fn is_regular(self) -> bool {
        self.denominator.is_power_of_two()
    }

    pub fn measure_ticks(self) -> Tick {
        let denominator = self.denominator.max(1) as Tick;
        let whole = self.numerator as Tick * TICKS_PER_QUARTER * 4;
        ((whole + denominator / 2) / denominator).max(1)
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

/// One point of a tempo curve. The curve is shared by the project, or each
/// track has its own when the project uses per-track automation.
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

/// Where a tie between two notes came from. The distinction decides what
/// happens when the editor switches from tab view back to brick view:
/// `BrickSplit` ties are re-joined into one brick, `User` ties are kept as
/// separate notes that merely sound continuously.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TieOrigin {
    /// The note is one slice of a brick that was cut at a barline.
    BrickSplit,
    /// The note was tied to its predecessor explicitly (Tie Note command or
    /// a tie imported from another format).
    User,
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
    /// `Some` marks this note as a continuation of the note on the same
    /// string that ends exactly where this one starts: it sounds as one
    /// sustained pitch with no new attack. Use
    /// `InstrumentTrack::is_tied_continuation` rather than reading this
    /// directly, since the flag alone doesn't prove a predecessor exists.
    #[serde(default)]
    pub tied_from_previous: Option<TieOrigin>,
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

    /// Whether this family has its own editor and playback path. The others
    /// stay selectable in the picker and build a Stringed track underneath.
    pub fn is_implemented(self) -> bool {
        matches!(self, InstrumentFamily::Stringed)
    }
}

/// Sub-type of a `Stringed` track: the defaults a new track starts with, and
/// the clef its notes are written in.
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

    /// General MIDI program that selects this variant's default sound.
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

/// Repeat signs on one bar of a track, keyed by the bar's first tick.
/// `repeat_close` is how many extra times the region ends here is played
/// (0 = no close sign), so a plain `|:  :|` plays its bars twice.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BarMark {
    pub at_tick: Tick,
    pub repeat_open: bool,
    pub repeat_close: u16,
}

impl BarMark {
    pub fn is_empty(&self) -> bool {
        !self.repeat_open && self.repeat_close == 0
    }
}

/// A plug-in format BetterWriter can list. Only some can be hosted.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PluginFormat {
    #[default]
    Vst3,
    Vst2,
    Clap,
    Lv2,
    Au,
}

impl PluginFormat {
    pub const ALL: [PluginFormat; 5] = [
        PluginFormat::Vst3,
        PluginFormat::Vst2,
        PluginFormat::Clap,
        PluginFormat::Lv2,
        PluginFormat::Au,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PluginFormat::Vst3 => "VST3",
            PluginFormat::Vst2 => "VST",
            PluginFormat::Clap => "CLAP",
            PluginFormat::Lv2 => "LV2",
            PluginFormat::Au => "AU",
        }
    }
}

/// One plug-in slot of a sound chain. The chain is saved with the project;
/// `state` is the plug-in's own saved settings, empty until it has run.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoundChainSlot {
    /// Names this instance while the program runs and across saves; 0 until
    /// `fresh_slot_id` gave it one.
    #[serde(default)]
    pub id: u64,
    pub name: String,
    pub path: String,
    pub is_instrument: bool,
    pub bypassed: bool,
    #[serde(default)]
    pub format: PluginFormat,
    #[serde(default)]
    pub vendor: String,
    /// Which class of a bundle that holds several, as hex; empty for the
    /// bundle's first.
    #[serde(default)]
    pub class_id: String,
    /// A name the user gave this instance; empty keeps `name`.
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub state: Vec<u8>,
}

/// A number no other slot has: the clock in nanoseconds, kept increasing
/// within a run so two slots made in the same instant still differ.
pub fn fresh_slot_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |elapsed| elapsed.as_nanos() as u64);
    let mut last = LAST.load(Ordering::Relaxed);
    loop {
        let next = now.max(last + 1);
        match LAST.compare_exchange(last, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(seen) => last = seen,
        }
    }
}

impl SoundChainSlot {
    pub fn display_name(&self) -> &str {
        if self.label.is_empty() { &self.name } else { &self.label }
    }

    /// "VST3i: Name", the way the chain lists it.
    pub fn list_label(&self) -> String {
        let tag = match (self.format, self.is_instrument) {
            (PluginFormat::Vst3, true) => "VST3i",
            (format, true) => return format!("{}i: {}", format.label(), self.display_name()),
            (format, false) => format.label(),
        };
        format!("{tag}: {}", self.display_name())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InstrumentTrack {
    pub id: u64,
    pub name: String,
    pub channel: u8,
    pub program: u8,
    /// Soundfont bank of `program`; 128 holds the drum kits.
    #[serde(default)]
    pub bank: u16,
    /// The sound follows the instrument (`instrument_id`) instead of the
    /// picked `bank` and `program`.
    #[serde(default)]
    pub sound_auto: bool,
    /// Id of the instrument in `core::instruments`; empty in files that
    /// only know a `stringed_variant`.
    #[serde(default)]
    pub instrument_id: String,
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
    /// Explicit number of bars in this track, independent of where the last
    /// note ends. Saved with the project so empty trailing bars survive a
    /// round trip, and so Insert/Add/Delete Bar have something to operate on.
    pub bar_count: u32,
    pub instrument_family: InstrumentFamily,
    pub stringed_variant: StringedVariant,
    /// Repeat signs, at most one entry per bar.
    #[serde(default)]
    pub bar_marks: Vec<BarMark>,
    /// Plug-ins the track's sound passes through, in order.
    #[serde(default)]
    pub sound_chain: Vec<SoundChainSlot>,
    /// The whole chain is switched off: the track plays without its plug-ins.
    #[serde(default)]
    pub fx_bypassed: bool,
    /// This track's own automation, used instead of the project's curves
    /// while `BwxProject::per_track_automation` is on. An empty tempo curve
    /// falls back to the project's.
    #[serde(default)]
    pub tempo_points: Vec<TempoPoint>,
    #[serde(default)]
    pub volume_points: Vec<AutomationPoint>,
    #[serde(default)]
    pub pan_points: Vec<AutomationPoint>,
}

impl InstrumentTrack {
    /// The instrument the track is, falling back to the one its tab-view
    /// type stands for when the file named none.
    pub fn instrument_def(&self) -> Option<&'static crate::core::instruments::InstrumentDef> {
        use crate::core::{instruments::instrument, sounds::default_instrument_id};
        instrument(&self.instrument_id)
            .or_else(|| instrument(default_instrument_id(self.stringed_variant)))
    }

    /// Bank and program the synth plays the track with.
    pub fn sound(&self) -> (u16, u8) {
        match self.instrument_def() {
            Some(def) if self.sound_auto => (def.bank, def.program),
            _ => (self.bank, self.program),
        }
    }

    /// The preset one note sounds with when it is not the track's usual one:
    /// palm muted, slapped or popped notes take the instrument's own preset
    /// for that technique. Only while the sound is automatic.
    pub fn note_sound(&self, note: &TabNote) -> Option<(u16, u8)> {
        use crate::core::instruments::{Technique, technique_sound};
        if !self.sound_auto {
            return None;
        }
        let effects = &note.effects;
        let technique = if effects.slapping {
            Technique::Slap
        } else if effects.popping {
            Technique::Pop
        } else if effects.palm_mute {
            Technique::Mute
        } else {
            return None;
        };
        technique_sound(self.instrument_def()?.id, technique).filter(|&sound| sound != self.sound())
    }

    /// The preset the track is sounding at `tick`: the one the latest note
    /// started by then asked for, else the track's usual one.
    pub fn sound_at(&self, tick: Tick) -> (u16, u8) {
        self.notes
            .iter()
            .filter(|note| note.abs_tick <= tick && !self.is_tied_continuation(note))
            .max_by_key(|note| note.abs_tick)
            .and_then(|note| self.note_sound(note))
            .unwrap_or_else(|| self.sound())
    }

    /// Makes the track another instrument: its tab-view type follows, and so
    /// does the sound while it is automatic. A sound that was picked by hand
    /// stays, as do the tuning and the notes.
    pub fn set_instrument(&mut self, def: &'static crate::core::instruments::InstrumentDef) {
        self.instrument_id = def.id.to_owned();
        self.stringed_variant = crate::core::sounds::variant_for_instrument(def);
        if self.sound_auto {
            self.bank = def.bank;
            self.program = def.program;
        }
    }

    /// Picks a sound by hand.
    pub fn set_sound(&mut self, bank: u16, program: u8) {
        self.sound_auto = false;
        self.bank = bank;
        self.program = program;
    }

    /// Lets the instrument choose the sound again.
    pub fn set_sound_automatic(&mut self) {
        self.sound_auto = true;
        if let Some(def) = self.instrument_def() {
            self.bank = def.bank;
            self.program = def.program;
        }
    }

    pub fn standard_guitar(id: u64, name: impl Into<String>, channel: u8) -> Self {
        Self {
            id,
            name: name.into(),
            channel,
            program: 24,
            bank: 0,
            sound_auto: false,
            instrument_id: "guitar-steel".to_owned(),
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
            bar_marks: Vec::new(),
            sound_chain: Vec::new(),
            fx_bypassed: false,
            tempo_points: Vec::new(),
            volume_points: Vec::new(),
            pan_points: Vec::new(),
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
            bank: 0,
            sound_auto: true,
            instrument_id: crate::core::sounds::default_instrument_id(stringed_variant).to_owned(),
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
            bar_marks: Vec::new(),
            sound_chain: Vec::new(),
            fx_bypassed: false,
            tempo_points: Vec::new(),
            volume_points: Vec::new(),
            pan_points: Vec::new(),
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

    /// The note this one is tied to: same string, ends exactly where `note`
    /// starts, same fret. `None` if `note` isn't flagged as tied or no such
    /// predecessor exists (e.g. it was deleted).
    pub fn tie_predecessor(&self, note: &TabNote) -> Option<&TabNote> {
        note.tied_from_previous?;
        self.notes.iter().find(|other| {
            other.id != note.id
                && other.string_index == note.string_index
                && other.fret == note.fret
                && other.end_tick() == note.abs_tick
        })
    }

    /// Whether `note` is a real tie continuation: flagged as tied *and*
    /// still backed by a matching predecessor.
    pub fn is_tied_continuation(&self, note: &TabNote) -> bool {
        self.tie_predecessor(note).is_some()
    }

    pub fn midi_key_for(&self, note: &TabNote) -> u8 {
        self.tuning
            .get(note.string_index)
            .copied()
            .unwrap_or(40)
            .saturating_add(note.fret)
            .min(127)
    }

    /// Stretches of palm muting as (first onset, last onset): consecutive
    /// onsets that carry the effect. An onset without it ends the stretch;
    /// rests do not.
    pub fn palm_mute_runs(&self) -> Vec<(Tick, Tick)> {
        let mut onsets: Vec<(Tick, bool)> = Vec::new();
        let mut sorted: Vec<&TabNote> = self.notes.iter().collect();
        sorted.sort_by_key(|note| note.abs_tick);
        for note in sorted {
            match onsets.last_mut() {
                Some((tick, muted)) if *tick == note.abs_tick => *muted |= note.effects.palm_mute,
                _ => onsets.push((note.abs_tick, note.effects.palm_mute)),
            }
        }
        let mut runs: Vec<(Tick, Tick)> = Vec::new();
        let mut open: Option<(Tick, Tick)> = None;
        for (tick, muted) in onsets {
            match (muted, open.as_mut()) {
                (true, Some(run)) => run.1 = tick,
                (true, None) => open = Some((tick, tick)),
                (false, _) => runs.extend(open.take()),
            }
        }
        runs.extend(open);
        runs
    }

    /// The tick where each palm muting stretch of `palm_mute_runs` stops
    /// sounding: the end of the longest muted note on its last onset.
    pub fn palm_mute_ends(&self) -> Vec<Tick> {
        self.palm_mute_runs()
            .into_iter()
            .map(|(_, last)| {
                self.notes
                    .iter()
                    .filter(|note| note.abs_tick == last && note.effects.palm_mute)
                    .map(|note| note.end_tick())
                    .max()
                    .unwrap_or(last)
            })
            .collect()
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
    /// timeline. This is the real, save/load-stable extent of the track -
    /// distinct from wherever the notes happen to currently reach.
    pub fn bars_end_tick(&self) -> Tick {
        let mut cursor = 0;
        for _ in 0..self.bar_count.max(1) {
            let (_, end) = self.measure_bounds_at(cursor);
            cursor = end;
        }
        cursor
    }

    /// Appends bars until the track's bars reach at least `end_tick`, the
    /// way pasting past the last bar does. Returns how many were added.
    pub fn grow_bars_to_cover(&mut self, end_tick: Tick) -> u32 {
        let mut added = 0;
        while self.bars_end_tick() < end_tick {
            self.bar_count = self.bar_count.max(1) + 1;
            added += 1;
        }
        added
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
    /// Shared tempo curve. Always has at least one point; the point at or
    /// before tick 0 governs playback until the next point takes over.
    pub tempo_points: Vec<TempoPoint>,
    /// Shared volume and pan curves, applied to every track. They are empty
    /// until a point is added; before the first point a track plays at its
    /// own volume and pan.
    #[serde(default)]
    pub volume_points: Vec<AutomationPoint>,
    #[serde(default)]
    pub pan_points: Vec<AutomationPoint>,
    /// Each track follows its own tempo, volume and pan curves instead of the
    /// shared ones.
    #[serde(default)]
    pub per_track_automation: bool,
    pub tracks: Vec<InstrumentTrack>,
    pub next_note_id: u64,
    /// Plug-ins the mix of every track passes through last.
    #[serde(default)]
    pub master_chain: Vec<SoundChainSlot>,
    #[serde(default)]
    pub master_fx_bypassed: bool,
}

impl BwxProject {
    /// Gives every plug-in slot of a freshly loaded project an id of its own.
    pub fn ensure_slot_ids(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let chains = self
            .tracks
            .iter_mut()
            .map(|track| &mut track.sound_chain)
            .chain(std::iter::once(&mut self.master_chain));
        for chain in chains {
            for slot in chain.iter_mut() {
                if slot.id == 0 || !seen.insert(slot.id) {
                    slot.id = fresh_slot_id();
                    seen.insert(slot.id);
                }
            }
        }
    }

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
                tied_from_previous: None,
            },
            TabNote {
                id: 2,
                abs_tick: TICKS_PER_QUARTER,
                duration_ticks: TICKS_PER_QUARTER / 2,
                string_index: 4,
                fret: 2,
                velocity: 108,
                effects: NoteEffects::default(),
                tied_from_previous: None,
            },
            TabNote {
                id: 3,
                abs_tick: TICKS_PER_QUARTER + TICKS_PER_QUARTER / 2,
                duration_ticks: TICKS_PER_QUARTER / 2,
                string_index: 3,
                fret: 2,
                velocity: 108,
                effects: NoteEffects::default(),
                tied_from_previous: None,
            },
            TabNote {
                id: 4,
                abs_tick: TICKS_PER_QUARTER * 2,
                duration_ticks: TICKS_PER_QUARTER,
                string_index: 2,
                fret: 1,
                velocity: 108,
                effects: NoteEffects::default(),
                tied_from_previous: None,
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
                tied_from_previous: None,
            },
            TabNote {
                id: 6,
                abs_tick: TICKS_PER_QUARTER / 2,
                duration_ticks: TICKS_PER_QUARTER / 2,
                string_index: 4,
                fret: 5,
                velocity: 94,
                effects: NoteEffects::default(),
                tied_from_previous: None,
            },
            TabNote {
                id: 7,
                abs_tick: TICKS_PER_QUARTER,
                duration_ticks: TICKS_PER_QUARTER / 2,
                string_index: 3,
                fret: 4,
                velocity: 94,
                effects: NoteEffects::default(),
                tied_from_previous: None,
            },
        ];

        Self {
            schema_version: 1,
            title: "BetterWriter sketch".to_owned(),
            tempo_points: vec![TempoPoint::new(0, 120.0)],
            volume_points: Vec::new(),
            pan_points: Vec::new(),
            per_track_automation: false,
            tracks: vec![guitar, baritone],
            next_note_id: 8,
            master_chain: Vec::new(),
            master_fx_bypassed: false,
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
            volume_points: Vec::new(),
            pan_points: Vec::new(),
            per_track_automation: false,
            tracks: vec![track],
            next_note_id: 1,
            master_chain: Vec::new(),
            master_fx_bypassed: false,
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

    /// Appends an empty track shaped like `source_id`: same instrument type,
    /// tuning, sound, bars and time signatures, but no notes and a fresh
    /// name, id and channel. Returns the new track's id.
    pub fn add_track_like(&mut self, source_id: u64) -> Option<u64> {
        let source = self.track(source_id)?;
        let id = self.tracks.iter().map(|track| track.id).max().unwrap_or(0) + 1;
        let base = source
            .instrument_def()
            .map_or_else(|| source.stringed_variant.label().to_owned(), |def| def.name.to_owned());
        let name = (2..)
            .map(|n| format!("{base} {n}"))
            .find(|candidate| self.tracks.iter().all(|track| track.name != *candidate))?;
        let mut track = source.clone();
        track.id = id;
        track.name = name;
        track.channel = crate::core::sounds::melodic_channel(self.tracks.len());
        track.solo = false;
        track.mute = false;
        track.notes.clear();
        self.tracks.push(track);
        Some(id)
    }

    pub fn track(&self, id: u64) -> Option<&InstrumentTrack> {
        self.tracks.iter().find(|track| track.id == id)
    }

    pub fn track_mut(&mut self, id: u64) -> Option<&mut InstrumentTrack> {
        self.tracks.iter_mut().find(|track| track.id == id)
    }

    /// Moves a track to place `to` (0-based, counted after the move) in the
    /// track order. The track keeps its id, channel and everything else, so
    /// only its row in the table, its timeline row and its place in exports
    /// change. Returns whether the order changed.
    pub fn move_track(&mut self, track_id: u64, to: usize) -> bool {
        let Some(from) = self.tracks.iter().position(|track| track.id == track_id) else {
            return false;
        };
        let to = to.min(self.tracks.len() - 1);
        if from == to {
            return false;
        }
        let track = self.tracks.remove(from);
        self.tracks.insert(to, track);
        true
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

#[cfg(test)]
mod tests {
    #[test]
    fn automatic_sound_follows_the_technique_of_each_note() {
        use crate::core::instruments::instrument;
        let mut project = BwxProject::demo();
        let id = project.tracks[0].id;
        let note = project.insert_note(id, 0, TICKS_PER_QUARTER, 0, 3).unwrap();
        let track = project.track_mut(id).unwrap();
        track.set_instrument(instrument("electric-guitar").unwrap());
        track.sound_auto = true;
        let mut played = track.notes.iter().find(|n| n.id == note).unwrap().clone();
        assert_eq!(track.note_sound(&played), None);
        played.effects.palm_mute = true;
        assert_eq!(track.note_sound(&played), Some((0, 28)));
        // A technique the instrument has no preset for changes nothing.
        played.effects.slapping = true;
        assert_eq!(track.note_sound(&played), None);
        track.set_instrument(instrument("electric-bass").unwrap());
        assert_eq!(track.note_sound(&played), Some((0, 36)));
        played.effects.slapping = false;
        played.effects.popping = true;
        assert_eq!(track.note_sound(&played), Some((0, 37)));
        // A sound picked by hand stays for every note.
        track.set_sound(0, 30);
        assert_eq!(track.note_sound(&played), None);
    }

    #[test]
    fn the_sound_at_the_playhead_is_that_of_the_latest_started_note() {
        use crate::core::instruments::instrument;
        let mut project = BwxProject::demo();
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        project.insert_note(id, 0, TICKS_PER_QUARTER, 0, 3).unwrap();
        let muted = project
            .insert_note(id, TICKS_PER_QUARTER * 2, TICKS_PER_QUARTER, 0, 5)
            .unwrap();
        let track = project.track_mut(id).unwrap();
        track.set_instrument(instrument("electric-guitar").unwrap());
        track.set_sound_automatic();
        track.notes.iter_mut().find(|n| n.id == muted).unwrap().effects.palm_mute = true;
        let usual = track.sound();
        assert_eq!(track.sound_at(0), usual);
        assert_eq!(track.sound_at(TICKS_PER_QUARTER), usual);
        assert_eq!(track.sound_at(TICKS_PER_QUARTER * 2), (0, 28));
        // The preset holds through the rest after the muted note.
        assert_eq!(track.sound_at(TICKS_PER_QUARTER * 3), (0, 28));
    }

    #[test]
    fn palm_muting_runs_from_the_first_to_the_last_muted_onset() {
        let q = TICKS_PER_QUARTER;
        let mut project = BwxProject::demo();
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        for (index, muted) in [true, true, false, true, false, true].into_iter().enumerate() {
            let note = project.insert_note(id, q * index as Tick, q, 0, 3).unwrap();
            let track = project.track_mut(id).unwrap();
            track.notes.iter_mut().find(|n| n.id == note).unwrap().effects.palm_mute = muted;
        }
        // A second, unmuted note on a muted onset does not break the run.
        project.insert_note(id, 0, q, 1, 2).unwrap();
        let runs = project.track(id).unwrap().palm_mute_runs();
        assert_eq!(runs, [(0, q), (q * 3, q * 3), (q * 5, q * 5)]);
        // Each stretch lasts to the end of its last muted note.
        assert_eq!(
            project.track(id).unwrap().palm_mute_ends(),
            [q * 2, q * 4, q * 6]
        );
        project.tracks[0].notes.clear();
        assert!(project.tracks[0].palm_mute_runs().is_empty());
    }

    #[test]
    fn irrational_signatures_have_whole_tick_bars() {
        assert_eq!(TimeSignature::new(4, 6).measure_ticks(), 2560);
        assert_eq!(TimeSignature::new(5, 12).measure_ticks(), 1600);
        // 3840 / 7 is 548.57: rounded to the nearest tick.
        assert_eq!(TimeSignature::new(1, 7).measure_ticks(), 549);
        assert_eq!(TimeSignature::new(3, 4).measure_ticks(), 2880);
        assert!(TimeSignature::new(3, 4).is_regular());
        assert!(!TimeSignature::new(4, 6).is_regular());
    }

    #[test]
    fn an_irrational_bar_blocks_standard_exports() {
        let mut project = BwxProject::demo();
        let track_id = project.tracks[0].id;
        let report = crate::core::CompatibilityReport::analyze(&project, 20_000);
        assert!(!report.has_irrational_meter);
        for track in &mut project.tracks {
            track.time_signature_changes.clear();
        }
        assert!(project.set_track_time_signature(track_id, 0, TimeSignature::new(5, 12)));
        let report = crate::core::CompatibilityReport::analyze(&project, 20_000);
        assert!(report.has_irrational_meter);
        assert!(!report.standard_exports_enabled);
        assert!(project.track(track_id).unwrap().bar_spans()[0].end == 1600);
    }

    use super::*;

    /// Where playback ends over every track.
    fn playback_end(project: &BwxProject) -> Option<Tick> {
        project
            .tracks
            .iter()
            .filter_map(InstrumentTrack::playback_end_tick)
            .max()
    }

    #[test]
    fn playback_ends_with_the_last_bar_that_holds_a_note() {
        let mut project = BwxProject::demo();
        // Demo guitar notes end three quarters into a 4/4 bar; the 5/8 track
        // ends inside its first bar.
        assert_eq!(playback_end(&project), Some(TICKS_PER_QUARTER * 4));

        project.tracks[0].bar_count = 3;
        project.insert_note(1, TICKS_PER_QUARTER * 9, TICKS_PER_QUARTER, 0, 1);
        assert_eq!(playback_end(&project), Some(TICKS_PER_QUARTER * 12));
    }

    #[test]
    fn a_note_ending_on_a_barline_belongs_to_the_bar_it_ends() {
        let mut project = BwxProject::demo();
        project.tracks[0].notes.clear();
        project.tracks[1].notes.clear();
        project.insert_note(1, 0, TICKS_PER_QUARTER * 4, 0, 1);
        assert_eq!(playback_end(&project), Some(TICKS_PER_QUARTER * 4));
    }

    #[test]
    fn a_project_without_notes_has_no_playback_end() {
        let project = BwxProject::empty_with_track(
            "t",
            InstrumentFamily::Stringed,
            StringedVariant::Bass,
            4,
        );
        assert_eq!(playback_end(&project), None);
    }

    #[test]
    fn a_new_track_copies_the_shape_but_not_the_notes() {
        let mut project = BwxProject::demo();
        let source = project.tracks[1].clone();
        let id = project.add_track_like(source.id).unwrap();

        let added = project.track(id).unwrap();
        assert_eq!(id, 3);
        assert!(added.notes.is_empty());
        assert_eq!(added.tuning, source.tuning);
        assert_eq!(added.time_signature_changes, source.time_signature_changes);
        assert_eq!(added.bar_count, source.bar_count);
        assert_eq!(added.channel, 2);
        assert_ne!(added.name, source.name);
        assert_eq!(project.tracks.len(), 3);
    }

    #[test]
    fn new_track_names_never_repeat() {
        let mut project = BwxProject::demo();
        let first = project.add_track_like(1).unwrap();
        let second = project.add_track_like(1).unwrap();
        assert_ne!(
            project.track(first).unwrap().name,
            project.track(second).unwrap().name
        );
    }

    #[test]
    fn a_new_track_chooses_its_sound_automatically() {
        let track = InstrumentTrack::new_track(
            1,
            "Bass",
            0,
            InstrumentFamily::Stringed,
            StringedVariant::Bass,
            4,
        );
        assert!(track.sound_auto);
        assert_eq!(track.sound(), (0, 33));
    }

    #[test]
    fn an_automatic_sound_follows_the_instrument() {
        let mut track = InstrumentTrack::new_track(
            1,
            "Guitar",
            0,
            InstrumentFamily::Stringed,
            StringedVariant::AcousticGuitar,
            6,
        );
        let flute = crate::core::instruments::instrument("flute").unwrap();
        track.set_instrument(flute);
        assert_eq!(track.instrument_id, "flute");
        assert_eq!(track.sound(), (flute.bank, flute.program));
        assert_eq!(track.stringed_variant, StringedVariant::Other);
    }

    #[test]
    fn a_picked_sound_survives_a_change_of_instrument() {
        let mut project = BwxProject::demo();
        let track = &mut project.tracks[0];
        track.set_sound(8, 25);
        let notes = track.notes.clone();
        let tuning = track.tuning.clone();
        let bass = crate::core::instruments::instrument("bass-guitar").unwrap();
        track.set_instrument(bass);
        assert_eq!(track.sound(), (8, 25));
        assert_eq!(track.stringed_variant, StringedVariant::Bass);
        assert_eq!(track.notes, notes);
        assert_eq!(track.tuning, tuning);
        track.set_sound_automatic();
        assert_eq!(track.sound(), (bass.bank, bass.program));
    }

    #[test]
    fn a_track_saved_without_an_instrument_id_uses_its_tab_type() {
        let mut track = BwxProject::demo().tracks[0].clone();
        track.instrument_id.clear();
        track.stringed_variant = StringedVariant::ElectricGuitar;
        assert_eq!(track.instrument_def().unwrap().id, "electric-guitar");
        assert_eq!(track.sound(), (track.bank, track.program));
    }

    #[test]
    fn a_new_track_starts_unmuted() {
        let mut project = BwxProject::demo();
        project.tracks[0].mute = true;
        project.tracks[0].solo = true;
        let id = project.add_track_like(1).unwrap();
        let added = project.track(id).unwrap();
        assert!(!added.mute && !added.solo);
    }
}

#[cfg(test)]
mod grow_bars_tests {
    use super::*;

    #[test]
    fn bars_grow_until_they_cover_the_tick() {
        let mut project = BwxProject::demo();
        let track = &mut project.tracks[0];
        track.bar_count = 2;
        let end = track.bars_end_tick();
        assert_eq!(track.grow_bars_to_cover(end), 0);
        assert_eq!(track.grow_bars_to_cover(end + 1), 1);
        assert_eq!(track.bar_count, 3);
        assert!(track.bars_end_tick() > end);
        assert_eq!(track.grow_bars_to_cover(track.bars_end_tick() + 1), 1);
    }
}

#[cfg(test)]
mod move_track_tests {
    use super::*;

    fn order(project: &BwxProject) -> Vec<u64> {
        project.tracks.iter().map(|track| track.id).collect()
    }

    #[test]
    fn a_track_moves_to_the_place_asked_for_and_keeps_its_content() {
        let mut project = BwxProject::demo();
        let first_id = project.tracks[0].id;
        project.add_track_like(first_id).unwrap();
        let ids = order(&project);
        assert!(ids.len() >= 3);
        let first = project.tracks[0].clone();

        assert!(project.move_track(ids[0], 2));
        let mut expected = ids.clone();
        expected.remove(0);
        expected.insert(2, ids[0]);
        assert_eq!(order(&project), expected);
        assert_eq!(project.tracks[2], first);

        // Moving it back restores the order; so does asking past the end.
        assert!(project.move_track(ids[0], 0));
        assert_eq!(order(&project), ids);
        assert!(project.move_track(ids[0], 99));
        assert_eq!(*order(&project).last().unwrap(), ids[0]);
    }

    #[test]
    fn moving_a_track_where_it_is_or_one_that_is_missing_changes_nothing() {
        let mut project = BwxProject::demo();
        let ids = order(&project);
        assert!(!project.move_track(ids[1], 1));
        assert!(!project.move_track(u64::MAX, 0));
        assert_eq!(order(&project), ids);
    }
}
