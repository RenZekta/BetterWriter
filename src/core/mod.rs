pub mod automation;
pub mod bars;
pub mod clipboard;
pub mod edit;
pub mod ghost;
pub mod instruments;
pub mod keyboard_entry;
pub mod length;
pub mod project;
pub mod shadow;
pub mod smf;
pub mod sounds;
pub mod tempo;
pub mod ties;
pub mod validation;

pub use automation::{AutomationLane, AutomationPoint, Scope};
pub use clipboard::NoteClip;
pub use edit::{RestFill, StretchEdge};
pub use project::{
    BwxProject, DurationChoice, InstrumentFamily, InstrumentTrack, NoteEffects,
    PluginFormat, SoundChainSlot, fresh_slot_id, StringedVariant, TICKS_PER_QUARTER, TempoPoint, TempoTransition, Tick,
    TimeSignature,
};
pub use shadow::{MidiEventKind, MidiPlaybackEvent, ShadowTimeline};
pub use tempo::TempoMap;
pub use validation::CompatibilityReport;
