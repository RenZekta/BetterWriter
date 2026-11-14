//! Sounds and mixer mapping for tracks: the General MIDI sound names the
//! bundled soundfont uses, the sounds offered per instrument type, and how
//! tracks map onto MIDI channels, volume and pan.

use super::project::StringedVariant;

/// MIDI channel index the synth reserves for percussion. A melodic track on
/// it would play as a drum kit whatever its program says.
pub const DRUM_CHANNEL: u8 = 9;

const LAST_CHANNEL: usize = 15;

/// Bank 0 preset names of `MS Basic.sf3`, indexed by General MIDI program.
const SOUND_NAMES: [&str; 128] = [
    "Grand Piano", "Bright Grand Piano", "Electric Grand", "Honky-Tonk Piano",
    "Tine Electric Piano", "FM Electric Piano", "Harpsichord", "Clavinet",
    "Celesta", "Glockenspiel", "Music Box", "Vibraphone",
    "Marimba", "Xylophone", "Tubular Bells", "Dulcimer",
    "Drawbar Organ", "Percussive Organ", "Rock Organ", "Church Organ",
    "Reed Organ", "Accordion", "Harmonica", "Bandoneon",
    "Nylon String Guitar", "Steel String Guitar", "Jazz Guitar", "Clean Guitar",
    "Palm Muted Guitar", "Overdrive Guitar", "Distortion Guitar", "Guitar Harmonics",
    "Acoustic Bass", "Fingered Bass", "Picked Bass", "Fretless Bass",
    "Slap Bass", "Pop Bass", "Synth Bass 1", "Synth Bass 2",
    "Violin", "Viola", "Cello", "Contrabass",
    "Strings Tremolo", "Strings Pizzicato", "Harp", "Timpani",
    "Strings Fast", "Strings Slow", "Synth Strings 1", "Synth Strings 2",
    "Choir Aahs", "Voice Oohs", "Synth Voice", "Orchestra Hit",
    "Trumpet", "Trombone", "Tuba", "Harmon Mute Trumpet",
    "French Horns", "Brass Section", "Synth Brass 1", "Synth Brass 2",
    "Soprano Sax", "Alto Sax", "Tenor Sax", "Baritone Sax",
    "Oboe", "English Horn", "Bassoon", "Clarinet",
    "Piccolo", "Flute", "Recorder", "Pan Flute",
    "Bottle Chiff", "Shakuhachi", "Whistle", "Ocarina",
    "Square Lead", "Saw Lead", "Calliope Lead", "Chiffer Lead",
    "Charang", "Solo Vox", "5th Saw Wave", "Bass & Lead",
    "Fantasia", "Warm Pad", "Polysynth", "Space Voice",
    "Bowed Glass", "Metal Pad", "Halo Pad", "Sweep Pad",
    "Ice Rain", "Soundtrack", "Crystal", "Atmosphere",
    "Brightness", "Goblin", "Echo Drops", "Star Theme",
    "Sitar", "Banjo", "Shamisen", "Koto",
    "Kalimba", "Bagpipe", "Fiddle", "Shenai",
    "Tinker Bell", "Agogo", "Steel Drums", "Woodblock",
    "Taiko Drum", "Melodic Tom", "Synth Drum", "Reverse Cymbal",
    "Fret Noise", "Breath Noise", "Sea Shore", "Bird Tweet",
    "Telephone", "Helicopter", "Applause", "Gun Shot",
];

pub fn sound_name(program: u8) -> &'static str {
    SOUND_NAMES
        .get(program as usize)
        .copied()
        .unwrap_or("Unknown sound")
}

/// Sounds that suit an instrument type, in the order a picker lists them.
pub fn recommended_programs(variant: StringedVariant) -> &'static [u8] {
    match variant {
        StringedVariant::AcousticGuitar => &[24, 25, 26, 31, 105],
        StringedVariant::ElectricGuitar => &[27, 26, 28, 29, 30, 31],
        StringedVariant::Bass => &[32, 33, 34, 35, 36, 37, 38, 39, 43],
        StringedVariant::Other => &[
            24, 25, 46, 15, 104, 105, 106, 107, 108, 110, 45, 40, 41, 42, 43,
        ],
    }
}

/// Programs a picker offers for a track: every sound when `all` is set,
/// otherwise the ones for its type. A current program outside that list is
/// kept (first) so an imported or earlier choice never disappears.
pub fn sound_choices(variant: StringedVariant, current: u8, all: bool) -> Vec<u8> {
    if all {
        return (0..SOUND_NAMES.len() as u8).collect();
    }
    let mut choices = recommended_programs(variant).to_vec();
    if !choices.contains(&current) {
        choices.insert(0, current);
    }
    choices
}

/// The instrument type whose sound list contains a program, used to type
/// tracks that come from a format with no such notion.
pub fn variant_for_program(program: u8) -> StringedVariant {
    match program {
        27..=31 => StringedVariant::ElectricGuitar,
        32..=39 => StringedVariant::Bass,
        _ => StringedVariant::AcousticGuitar,
    }
}

/// MIDI channel for the `track_index`-th track. The drum channel is skipped,
/// and tracks past the last channel share it.
pub fn melodic_channel(track_index: usize) -> u8 {
    let channel = if track_index >= DRUM_CHANNEL as usize {
        track_index + 1
    } else {
        track_index
    };
    channel.min(LAST_CHANNEL) as u8
}

/// MIDI pan controller value (0 hard left, 64 centre, 127 hard right) for a
/// track pan stored as an offset from the centre.
pub fn midi_pan(pan: i8) -> u8 {
    (64_i16 + pan as i16).clamp(0, 127) as u8
}

pub fn pan_from_midi(value: u8) -> i8 {
    (value as i16 - 64).clamp(-64, 63) as i8
}

#[cfg(test)]
mod tests {
    use super::*;

    const VARIANTS: [StringedVariant; 4] = [
        StringedVariant::AcousticGuitar,
        StringedVariant::ElectricGuitar,
        StringedVariant::Bass,
        StringedVariant::Other,
    ];

    #[test]
    fn sound_names_follow_general_midi_order() {
        assert_eq!(sound_name(0), "Grand Piano");
        assert_eq!(sound_name(24), "Nylon String Guitar");
        assert_eq!(sound_name(33), "Fingered Bass");
        assert_eq!(sound_name(127), "Gun Shot");
        assert_eq!(sound_name(200), "Unknown sound");
    }

    #[test]
    fn every_type_lists_its_default_sound_and_only_real_programs() {
        for variant in VARIANTS {
            let list = recommended_programs(variant);
            assert!(list.contains(&variant.default_program()), "{variant:?}");
            assert!(list.iter().all(|program| *program < 128), "{variant:?}");
        }
    }

    #[test]
    fn the_full_list_has_every_sound_once() {
        let all = sound_choices(StringedVariant::Bass, 33, true);
        assert_eq!(all, (0..128).collect::<Vec<u8>>());
    }

    #[test]
    fn a_current_sound_outside_the_type_list_stays_selectable() {
        let choices = sound_choices(StringedVariant::Bass, 0, false);
        assert_eq!(choices[0], 0);
        assert_eq!(choices.len(), recommended_programs(StringedVariant::Bass).len() + 1);
        let in_list = sound_choices(StringedVariant::Bass, 33, false);
        assert_eq!(in_list, recommended_programs(StringedVariant::Bass));
    }

    #[test]
    fn tracks_are_typed_by_their_sound() {
        assert_eq!(variant_for_program(24), StringedVariant::AcousticGuitar);
        assert_eq!(variant_for_program(29), StringedVariant::ElectricGuitar);
        assert_eq!(variant_for_program(35), StringedVariant::Bass);
        assert_eq!(variant_for_program(0), StringedVariant::AcousticGuitar);
    }

    #[test]
    fn melodic_tracks_never_land_on_the_drum_channel() {
        let channels: Vec<u8> = (0..20).map(melodic_channel).collect();
        assert!(channels.iter().all(|channel| *channel != DRUM_CHANNEL));
        assert_eq!(&channels[..10], &[0, 1, 2, 3, 4, 5, 6, 7, 8, 10]);
        assert_eq!(channels[19], 15);
    }

    #[test]
    fn pan_round_trips_through_the_midi_range() {
        assert_eq!(midi_pan(0), 64);
        assert_eq!(midi_pan(-64), 0);
        assert_eq!(midi_pan(63), 127);
        for pan in -64..=63_i8 {
            assert_eq!(pan_from_midi(midi_pan(pan)), pan);
        }
        assert_eq!(pan_from_midi(127), 63);
        assert_eq!(pan_from_midi(0), -64);
    }
}
