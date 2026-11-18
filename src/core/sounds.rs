//! Sounds and mixer mapping for tracks: the presets of the bundled soundfont,
//! the menu they are offered in (the one MuseScore shows for `MS Basic`), and
//! how tracks map onto MIDI channels, volume and pan.

use super::instruments;
use super::project::StringedVariant;

/// MIDI channel index the synth reserves for percussion. A melodic track on
/// it would play as a drum kit whatever its program says.
pub const DRUM_CHANNEL: u8 = 9;

const LAST_CHANNEL: usize = 15;

/// One top-level entry of the sound menu: presets directly, or titled
/// groups of presets.
pub enum SoundMenu {
    Presets(&'static str, &'static [(u16, u8)]),
    Groups(&'static str, &'static [(&'static str, &'static [(u16, u8)])]),
}

/// Soundfont presets: bank, program and name, sorted by bank then program.
pub static PRESETS: &[(u16, u8, &str)] = &[
    (0, 0, "Grand Piano"),
    (0, 1, "Bright Grand Piano"),
    (0, 2, "Electric Grand"),
    (0, 3, "Honky-Tonk Piano"),
    (0, 4, "Tine Electric Piano"),
    (0, 5, "FM Electric Piano"),
    (0, 6, "Harpsichord"),
    (0, 7, "Clavinet"),
    (0, 8, "Celesta"),
    (0, 9, "Glockenspiel"),
    (0, 10, "Music Box"),
    (0, 11, "Vibraphone"),
    (0, 12, "Marimba"),
    (0, 13, "Xylophone"),
    (0, 14, "Tubular Bells"),
    (0, 15, "Dulcimer"),
    (0, 16, "Drawbar Organ"),
    (0, 17, "Percussive Organ"),
    (0, 18, "Rock Organ"),
    (0, 19, "Church Organ"),
    (0, 20, "Reed Organ"),
    (0, 21, "Accordion"),
    (0, 22, "Harmonica"),
    (0, 23, "Bandoneon"),
    (0, 24, "Nylon String Guitar"),
    (0, 25, "Steel String Guitar"),
    (0, 26, "Jazz Guitar"),
    (0, 27, "Clean Guitar"),
    (0, 28, "Palm Muted Guitar"),
    (0, 29, "Overdrive Guitar"),
    (0, 30, "Distortion Guitar"),
    (0, 31, "Guitar Harmonics"),
    (0, 32, "Acoustic Bass"),
    (0, 33, "Fingered Bass"),
    (0, 34, "Picked Bass"),
    (0, 35, "Fretless Bass"),
    (0, 36, "Slap Bass"),
    (0, 37, "Pop Bass"),
    (0, 38, "Synth Bass 1"),
    (0, 39, "Synth Bass 2"),
    (0, 40, "Violin"),
    (0, 41, "Viola"),
    (0, 42, "Cello"),
    (0, 43, "Contrabass"),
    (0, 44, "Strings Tremolo"),
    (0, 45, "Strings Pizzicato"),
    (0, 46, "Harp"),
    (0, 47, "Timpani"),
    (0, 48, "Strings Fast"),
    (0, 49, "Strings Slow"),
    (0, 50, "Synth Strings 1"),
    (0, 51, "Synth Strings 2"),
    (0, 52, "Choir Aahs"),
    (0, 53, "Voice Oohs"),
    (0, 54, "Synth Voice"),
    (0, 55, "Orchestra Hit"),
    (0, 56, "Trumpet"),
    (0, 57, "Trombone"),
    (0, 58, "Tuba"),
    (0, 59, "Harmon Mute Trumpet"),
    (0, 60, "French Horns"),
    (0, 61, "Brass Section"),
    (0, 62, "Synth Brass 1"),
    (0, 63, "Synth Brass 2"),
    (0, 64, "Soprano Sax"),
    (0, 65, "Alto Sax"),
    (0, 66, "Tenor Sax"),
    (0, 67, "Baritone Sax"),
    (0, 68, "Oboe"),
    (0, 69, "English Horn"),
    (0, 70, "Bassoon"),
    (0, 71, "Clarinet"),
    (0, 72, "Piccolo"),
    (0, 73, "Flute"),
    (0, 74, "Recorder"),
    (0, 75, "Pan Flute"),
    (0, 76, "Bottle Chiff"),
    (0, 77, "Shakuhachi"),
    (0, 78, "Whistle"),
    (0, 79, "Ocarina"),
    (0, 80, "Square Lead"),
    (0, 81, "Saw Lead"),
    (0, 82, "Calliope Lead"),
    (0, 83, "Chiffer Lead"),
    (0, 84, "Charang"),
    (0, 85, "Solo Vox"),
    (0, 86, "5th Saw Wave"),
    (0, 87, "Bass & Lead"),
    (0, 88, "Fantasia"),
    (0, 89, "Warm Pad"),
    (0, 90, "Polysynth"),
    (0, 91, "Space Voice"),
    (0, 92, "Bowed Glass"),
    (0, 93, "Metal Pad"),
    (0, 94, "Halo Pad"),
    (0, 95, "Sweep Pad"),
    (0, 96, "Ice Rain"),
    (0, 97, "Soundtrack"),
    (0, 98, "Crystal"),
    (0, 99, "Atmosphere"),
    (0, 100, "Brightness"),
    (0, 101, "Goblin"),
    (0, 102, "Echo Drops"),
    (0, 103, "Star Theme"),
    (0, 104, "Sitar"),
    (0, 105, "Banjo"),
    (0, 106, "Shamisen"),
    (0, 107, "Koto"),
    (0, 108, "Kalimba"),
    (0, 109, "Bagpipe"),
    (0, 110, "Fiddle"),
    (0, 111, "Shenai"),
    (0, 112, "Tinker Bell"),
    (0, 113, "Agogo"),
    (0, 114, "Steel Drums"),
    (0, 115, "Woodblock"),
    (0, 116, "Taiko Drum"),
    (0, 117, "Melodic Tom"),
    (0, 118, "Synth Drum"),
    (0, 119, "Reverse Cymbal"),
    (0, 120, "Fret Noise"),
    (0, 121, "Breath Noise"),
    (0, 122, "Sea Shore"),
    (0, 123, "Bird Tweet"),
    (0, 124, "Telephone"),
    (0, 125, "Helicopter"),
    (0, 126, "Applause"),
    (0, 127, "Gun Shot"),
    (1, 115, "Temple Blocks"),
    (8, 0, "Mellow Grand Piano"),
    (8, 4, "Detuned Tine EP"),
    (8, 5, "Detuned FM EP"),
    (8, 6, "Coupled Harpsichord"),
    (8, 14, "Church Bell"),
    (8, 16, "Detuned Organ 1"),
    (8, 17, "Detuned Organ 2"),
    (8, 19, "Church Organ 2"),
    (8, 21, "Italian Accordion"),
    (8, 24, "Ukulele"),
    (8, 25, "12-String Guitar"),
    (8, 26, "Hawaiian Guitar"),
    (8, 28, "Funk Guitar"),
    (8, 30, "Feedback Guitar"),
    (8, 31, "Guitar Feedback"),
    (8, 38, "Synth Bass 3"),
    (8, 39, "Synth Bass 4"),
    (8, 40, "Slow Violin"),
    (8, 48, "Orchestral Pad"),
    (8, 50, "Synth Strings 3"),
    (8, 61, "Brass 2"),
    (8, 62, "Synth Brass 3"),
    (8, 63, "Synth Brass 4"),
    (8, 80, "Sine Wave"),
    (8, 107, "Taisho Koto"),
    (8, 115, "Castanets"),
    (8, 116, "Concert Bass Drum"),
    (8, 117, "Melo Tom 2"),
    (8, 118, "808 Tom"),
    (16, 25, "Mandolin"),
    (17, 16, "Drawbar Organ Expr."),
    (17, 17, "Perc. Organ Expr."),
    (17, 18, "Rock Organ Expr."),
    (17, 19, "Church Organ Expr."),
    (17, 20, "Reed Organ Expr."),
    (17, 21, "Accordion Expr."),
    (17, 22, "Harmonica Expr."),
    (17, 23, "Bandoneon Expr."),
    (17, 40, "Violin Expr."),
    (17, 41, "Viola Expr."),
    (17, 42, "Cello Expr."),
    (17, 43, "Contrabass Expr."),
    (17, 44, "Strings Trem Expr."),
    (17, 48, "Strings Fast Expr."),
    (17, 49, "Strings Slow Expr."),
    (17, 50, "Syn. Strings 1 Expr."),
    (17, 51, "Syn. Strings 2 Expr."),
    (17, 52, "Choir Aahs Expr."),
    (17, 53, "Voice Oohs Expr."),
    (17, 54, "Synth Voice Expr."),
    (17, 56, "Trumpet Expr."),
    (17, 57, "Trombone Expr."),
    (17, 58, "Tuba Expr."),
    (17, 59, "Hmn. Mute Tpt. Expr."),
    (17, 60, "French Horns Expr."),
    (17, 61, "Brass Section Expr."),
    (17, 62, "Synth Brass 1 Expr."),
    (17, 63, "Synth Brass 2 Expr."),
    (17, 64, "Soprano Sax Expr."),
    (17, 65, "Alto Sax Expr."),
    (17, 66, "Tenor Sax Expr."),
    (17, 67, "Baritone Sax Expr."),
    (17, 68, "Oboe Expr."),
    (17, 69, "English Horn Expr."),
    (17, 70, "Bassoon Expr."),
    (17, 71, "Clarinet Expr."),
    (17, 72, "Piccolo Expr."),
    (17, 73, "Flute Expr."),
    (17, 74, "Recorder Expr."),
    (17, 75, "Pan Flute Expr."),
    (17, 76, "Bottle Chiff Expr."),
    (17, 77, "Shakuhachi Expr."),
    (17, 78, "Whistle Expr."),
    (17, 79, "Ocarina Expr."),
    (17, 80, "Square Lead Expr."),
    (17, 81, "Saw Lead Expr."),
    (17, 82, "Calliope Lead Expr."),
    (17, 83, "Chiffer Lead Expr."),
    (17, 84, "Charang Expr."),
    (17, 85, "Solo Vox Expr."),
    (17, 86, "5th Saw Wave Expr."),
    (17, 87, "Bass & Lead Expr."),
    (17, 89, "Warm Pad Expr."),
    (17, 90, "Polysynth Expr."),
    (17, 91, "Space Voice Expr."),
    (17, 92, "Bowed Glass Expr."),
    (17, 93, "Metal Pad Expr."),
    (17, 94, "Halo Pad Expr."),
    (17, 95, "Sweep Pad Expr."),
    (17, 97, "Soundtrack Expr."),
    (17, 99, "Atmosphere Expr."),
    (17, 101, "Goblin Expr."),
    (17, 102, "Echo Drops Expr."),
    (17, 103, "Star Theme Expr."),
    (17, 110, "Fiddle Expr."),
    (17, 111, "Shenai Expr."),
    (18, 16, "Detuned Org. 1 Expr."),
    (18, 17, "Detuned Org. 2 Expr."),
    (18, 19, "Church Organ 2 Expr."),
    (18, 21, "It. Accordion Expr."),
    (18, 40, "Slow Violin Expr."),
    (18, 50, "Synth Strings3 Expr."),
    (18, 61, "Brass 2 Expr."),
    (18, 62, "Synth Brass 3 Expr."),
    (18, 63, "Synth Brass 4 Expr."),
    (18, 80, "Sine Wave Expr."),
    (20, 44, "Violins Tremolo"),
    (20, 45, "Violins Pizzicato"),
    (20, 48, "Violins Fast"),
    (20, 49, "Violins Slow"),
    (20, 81, "Detuned Saw"),
    (21, 44, "Violins Trem Expr."),
    (21, 48, "Violins Fast Expr."),
    (21, 49, "Violins Slow Expr."),
    (21, 81, "Detuned Saw Expr."),
    (25, 44, "Violins2 Tremolo"),
    (25, 45, "Violins2 Pizzicato"),
    (25, 48, "Violins2 Fast"),
    (25, 49, "Violins2 Slow"),
    (26, 44, "Violins2 Trem Expr."),
    (26, 48, "Violins2 Fast Expr."),
    (26, 49, "Violins2 Slow Expr."),
    (30, 44, "Violas Tremolo"),
    (30, 45, "Violas Pizzicato"),
    (30, 48, "Violas Fast"),
    (30, 49, "Violas Slow"),
    (31, 44, "Violas Trem Expr."),
    (31, 48, "Violas Fast Expr."),
    (31, 49, "Violas Slow Expr."),
    (40, 44, "Celli Tremolo"),
    (40, 45, "Celli Pizzicato"),
    (40, 48, "Celli Fast"),
    (40, 49, "Celli Slow"),
    (41, 44, "Celli Trem Expr."),
    (41, 48, "Celli Fast Expr."),
    (41, 49, "Celli Slow Expr."),
    (50, 44, "Basses Tremolo"),
    (50, 45, "Basses Pizzicato"),
    (50, 48, "Basses Fast"),
    (50, 49, "Basses Slow"),
    (51, 44, "Basses Trem Expr."),
    (51, 48, "Basses Fast Expr."),
    (51, 49, "Basses Slow Expr."),
    (128, 0, "Standard"),
    (128, 1, "Standard 1"),
    (128, 2, "Standard 2"),
    (128, 3, "Standard 3"),
    (128, 4, "Standard 4"),
    (128, 5, "Standard 5"),
    (128, 6, "Standard 6"),
    (128, 7, "Standard 7"),
    (128, 8, "Room"),
    (128, 9, "Room 1"),
    (128, 10, "Room 2"),
    (128, 11, "Room 3"),
    (128, 12, "Room 4"),
    (128, 13, "Room 5"),
    (128, 14, "Room 6"),
    (128, 15, "Room 7"),
    (128, 16, "Power"),
    (128, 17, "Power 1"),
    (128, 18, "Power 2"),
    (128, 19, "Power 3"),
    (128, 24, "Electronic"),
    (128, 25, "TR-808"),
    (128, 32, "Jazz"),
    (128, 33, "Jazz 1"),
    (128, 34, "Jazz 2"),
    (128, 35, "Jazz 3"),
    (128, 36, "Jazz 4"),
    (128, 40, "Brush"),
    (128, 41, "Brush 1"),
    (128, 42, "Brush 2"),
    (128, 48, "Orchestra Kit"),
    (128, 56, "Marching Snare"),
    (128, 57, "OldMarchingBass"),
    (128, 58, "Marching Cymbals"),
    (128, 59, "Marching Bass"),
    (128, 95, "OldMarchingTenor"),
    (128, 96, "Marching Tenor"),
];

pub static MENU: &[SoundMenu] = &[
    SoundMenu::Presets("Pipe", &[(0, 72), (17, 72), (0, 73), (17, 73), (0, 74), (17, 74), (0, 75), (17, 75), (0, 76), (17, 76), (0, 77), (17, 77), (0, 78), (17, 78), (0, 79), (17, 79)]),
    SoundMenu::Presets("Reed", &[(0, 64), (17, 64), (0, 65), (17, 65), (0, 66), (17, 66), (0, 67), (17, 67), (0, 68), (17, 68), (0, 69), (17, 69), (0, 70), (17, 70), (0, 71), (17, 71)]),
    SoundMenu::Presets("Brass", &[(0, 56), (17, 56), (0, 57), (17, 57), (0, 58), (17, 58), (0, 59), (17, 59), (0, 60), (17, 60), (0, 61), (17, 61), (8, 61), (18, 61), (0, 62), (17, 62), (8, 62), (18, 62), (0, 63), (17, 63), (8, 63), (18, 63)]),
    SoundMenu::Presets("Vocal", &[(0, 52), (17, 52), (0, 53), (17, 53), (0, 54), (17, 54)]),
    SoundMenu::Presets("Piano", &[(0, 0), (8, 0), (0, 1), (0, 2), (0, 3), (0, 4), (8, 4), (0, 5), (8, 5), (0, 6), (8, 6), (0, 7)]),
    SoundMenu::Presets("Organ", &[(0, 16), (17, 16), (8, 16), (18, 16), (0, 17), (17, 17), (8, 17), (18, 17), (0, 18), (17, 18), (0, 19), (17, 19), (8, 19), (18, 19), (0, 20), (17, 20), (0, 21), (17, 21), (8, 21), (18, 21), (0, 22), (17, 22), (0, 23), (17, 23)]),
    SoundMenu::Presets("Guitar", &[(0, 24), (8, 24), (0, 25), (8, 25), (16, 25), (0, 26), (8, 26), (0, 27), (0, 28), (8, 28), (0, 29), (0, 30), (8, 30), (0, 31), (8, 31)]),
    SoundMenu::Presets("Bass", &[(0, 32), (0, 33), (0, 34), (0, 35), (0, 36), (0, 37), (0, 38), (8, 38), (0, 39), (8, 39)]),
    SoundMenu::Presets("Strings", &[(0, 40), (17, 40), (8, 40), (18, 40), (0, 41), (17, 41), (0, 42), (17, 42), (0, 43), (17, 43), (0, 44), (17, 44), (20, 44), (21, 44), (25, 44), (26, 44), (30, 44), (31, 44), (40, 44), (41, 44), (50, 44), (51, 44), (0, 45), (20, 45), (25, 45), (30, 45), (40, 45), (50, 45), (0, 46), (0, 47), (0, 48), (17, 48), (8, 48), (20, 48), (21, 48), (25, 48), (26, 48), (30, 48), (31, 48), (40, 48), (41, 48), (50, 48), (51, 48), (0, 49), (17, 49), (20, 49), (21, 49), (25, 49), (26, 49), (30, 49), (31, 49), (40, 49), (41, 49), (50, 49), (51, 49), (0, 50), (17, 50), (8, 50), (18, 50), (0, 51), (17, 51)]),
    SoundMenu::Presets("World", &[(0, 104), (0, 105), (0, 106), (0, 107), (8, 107), (0, 108), (0, 109), (0, 110), (17, 110), (0, 111), (17, 111)]),
    SoundMenu::Groups("Synth", &[
        ("Lead", &[(0, 80), (17, 80), (8, 80), (18, 80), (0, 81), (17, 81), (0, 82), (17, 82), (0, 83), (17, 83), (0, 84), (17, 84), (0, 85), (17, 85), (0, 86), (17, 86), (0, 87), (17, 87)]),
        ("Pad", &[(0, 88), (0, 89), (17, 89), (0, 90), (17, 90), (0, 91), (17, 91), (0, 92), (17, 92), (0, 93), (17, 93), (0, 94), (17, 94), (0, 95), (17, 95)]),
        ("Effects", &[(0, 96), (0, 97), (17, 97), (0, 98), (0, 99), (17, 99), (0, 100), (0, 101), (17, 101), (0, 102), (17, 102), (0, 103), (17, 103)]),
    ]),
    SoundMenu::Presets("Sound effects", &[(0, 120), (0, 121), (0, 122), (0, 123), (0, 124), (0, 125), (0, 126), (0, 127)]),
    SoundMenu::Presets("Percussion (chromatic)", &[(0, 8), (0, 9), (0, 10), (0, 11), (0, 12), (0, 13), (0, 14), (8, 14), (0, 15)]),
    SoundMenu::Presets("Percussion (mixed)", &[(0, 55), (0, 112), (0, 113), (0, 114), (0, 115), (1, 115), (8, 115), (0, 116), (8, 116), (0, 117), (8, 117), (0, 118), (8, 118), (0, 119)]),
    SoundMenu::Groups("Drumsets", &[
        ("Standard kits", &[(128, 0), (128, 1), (128, 2), (128, 3), (128, 4), (128, 5), (128, 6), (128, 7)]),
        ("Room kits", &[(128, 8), (128, 9), (128, 10), (128, 11), (128, 12), (128, 13), (128, 14), (128, 15)]),
        ("Power kits", &[(128, 16), (128, 17), (128, 18), (128, 19)]),
        ("Electronic kits", &[(128, 24)]),
        ("TR-808 kits", &[(128, 25)]),
        ("Jazz kits", &[(128, 32), (128, 33), (128, 34), (128, 35), (128, 36)]),
        ("Brush kits", &[(128, 40), (128, 41), (128, 42)]),
        ("Orchestra kits", &[(128, 48)]),
        ("Marching kits", &[(128, 56), (128, 57), (128, 58), (128, 59), (128, 95), (128, 96)]),
    ]),
];

/// A soundfont preset by bank and program.
pub fn preset_name(bank: u16, program: u8) -> Option<&'static str> {
    PRESETS
        .binary_search_by_key(&(bank, program), |(bank, program, _)| (*bank, *program))
        .ok()
        .map(|index| PRESETS[index].2)
}

/// What the menu and the track table call a sound.
pub fn sound_label(bank: u16, program: u8) -> String {
    preset_name(bank, program).map_or_else(
        || format!("Bank {bank}, preset {program}"),
        str::to_owned,
    )
}

/// The menu leaves out the expression variants of the organs and strings,
/// as MuseScore does.
pub fn offered_in_menu(bank: u16, program: u8) -> bool {
    preset_name(bank, program).is_some_and(|name| !name.contains("Expr."))
}

/// Every offered preset whose name or category holds `search`, as bank,
/// program and name, in menu order.
pub fn search_presets(search: &str) -> Vec<(u16, u8, &'static str)> {
    let needle = search.trim().to_lowercase();
    let mut found = Vec::new();
    let mut take = |category: &str, group: &str, list: &[(u16, u8)]| {
        for &(bank, program) in list {
            if !offered_in_menu(bank, program) {
                continue;
            }
            let name = preset_name(bank, program).unwrap_or_default();
            let haystack = format!("{category} {group} {name}").to_lowercase();
            if haystack.contains(&needle) {
                found.push((bank, program, name));
            }
        }
    };
    for entry in MENU {
        match entry {
            SoundMenu::Presets(title, list) => take(title, "", list),
            SoundMenu::Groups(title, groups) => {
                for (group, list) in *groups {
                    take(title, group, list);
                }
            }
        }
    }
    found
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

/// The instrument a track without a saved instrument id stands for.
pub fn default_instrument_id(variant: StringedVariant) -> &'static str {
    match variant {
        StringedVariant::AcousticGuitar => "guitar-steel",
        StringedVariant::ElectricGuitar => "electric-guitar",
        StringedVariant::Bass => "electric-bass",
        StringedVariant::Other => "guitar-nylon",
    }
}

/// The tab-view type that suits an instrument.
pub fn variant_for_instrument(def: &instruments::InstrumentDef) -> StringedVariant {
    if def.bass_clef || (def.is_stringed() && def.id.contains("bass")) {
        StringedVariant::Bass
    } else if def.id.starts_with("electric-guitar") {
        StringedVariant::ElectricGuitar
    } else if def.is_stringed() && def.id.contains("guitar") {
        StringedVariant::AcousticGuitar
    } else {
        StringedVariant::Other
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

    #[test]
    fn presets_are_sorted_so_lookups_find_them() {
        assert!(PRESETS.windows(2).all(|pair| (pair[0].0, pair[0].1) < (pair[1].0, pair[1].1)));
        assert_eq!(preset_name(0, 24), Some("Nylon String Guitar"));
        assert_eq!(preset_name(128, 0), Some("Standard"));
        assert_eq!(preset_name(5, 5), None);
        assert_eq!(sound_label(5, 5), "Bank 5, preset 5");
    }

    #[test]
    fn every_menu_entry_names_a_preset_of_the_soundfont() {
        for entry in MENU {
            let lists: Vec<&[(u16, u8)]> = match entry {
                SoundMenu::Presets(_, list) => vec![list],
                SoundMenu::Groups(_, groups) => groups.iter().map(|(_, list)| *list).collect(),
            };
            for list in lists {
                for &(bank, program) in list {
                    assert!(preset_name(bank, program).is_some(), "{bank}/{program}");
                }
            }
        }
    }

    #[test]
    fn expression_variants_stay_out_of_the_menu() {
        assert!(!offered_in_menu(17, 16));
        assert!(offered_in_menu(0, 16));
        assert!(search_presets("expr.").is_empty());
    }

    #[test]
    fn the_search_matches_names_and_categories() {
        let by_name = search_presets("nylon");
        assert!(by_name.iter().any(|(bank, program, _)| (*bank, *program) == (0, 24)));
        let by_category = search_presets("electronic kits");
        assert_eq!(by_category.len(), 1);
        assert_eq!(by_category[0].0, 128);
        assert!(search_presets("").len() > 200);
    }

    #[test]
    fn every_instruments_automatic_sound_exists_or_falls_back_to_bank_zero() {
        for def in instruments::INSTRUMENTS {
            let known = preset_name(def.bank, def.program).is_some()
                || preset_name(0, def.program).is_some();
            assert!(known, "{} {}/{}", def.id, def.bank, def.program);
        }
    }

    #[test]
    fn instruments_pick_their_tab_view_type() {
        let variant = |id: &str| variant_for_instrument(instruments::instrument(id).unwrap());
        assert_eq!(variant("electric-guitar"), StringedVariant::ElectricGuitar);
        assert_eq!(variant("bass-guitar"), StringedVariant::Bass);
        assert_eq!(variant("electric-bass"), StringedVariant::Bass);
        assert_eq!(variant("guitar-steel"), StringedVariant::AcousticGuitar);
        assert_eq!(variant("flute"), StringedVariant::Other);
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
