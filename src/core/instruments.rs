//! The instrument list of MuseScore (share/instruments/instruments.xml): what
//! a track can be, with the General MIDI program and bank that choose its
//! sound automatically. Genres, groups and instruments keep the order of the
//! source file, which is the order MuseScore lists them in.
//!
//! Generated from MuseScore's instruments.xml (GPL-3.0-only); accidentals in
//! names are spelled with `b` and `#`.

/// A genre filter of the instrument picker.
pub struct Genre {
    #[allow(dead_code)]
    pub id: &'static str,
    pub name: &'static str,
}

/// A family block of the instrument picker.
pub struct Group {
    pub id: &'static str,
    pub name: &'static str,
}

pub struct InstrumentDef {
    pub id: &'static str,
    pub name: &'static str,
    /// What tells apart instruments of one name (the transposition, say).
    pub trait_name: &'static str,
    /// Index into `GROUPS`.
    pub group: u8,
    /// Bit `n` is set when the instrument is listed under `GENRES[n]`.
    pub genres: u16,
    /// Soundfont bank and program of the automatic sound.
    pub bank: u16,
    pub program: u8,
    /// Plays a drum kit; kept for the day tracks can be percussion.
    #[allow(dead_code)]
    pub drumset: bool,
    /// Written in the bass clef.
    pub bass_clef: bool,
}

pub static GENRES: &[Genre] = &[
    Genre { id: "common", name: "Common" },
    Genre { id: "popular", name: "Pop/Rock" },
    Genre { id: "jazz", name: "Jazz" },
    Genre { id: "choral", name: "Choral" },
    Genre { id: "orchestra", name: "Orchestra" },
    Genre { id: "concertband", name: "Concert Band" },
    Genre { id: "marching", name: "Marching Band" },
    Genre { id: "electronic", name: "Electronic Music" },
    Genre { id: "world", name: "World Music" },
    Genre { id: "earlymusic", name: "Early Music" },
    Genre { id: "classroom", name: "Classroom" },
];

pub static GROUPS: &[Group] = &[
    Group { id: "woodwinds", name: "Woodwinds" },
    Group { id: "free-reed", name: "Free Reed" },
    Group { id: "brass", name: "Brass" },
    Group { id: "pitched-percussion", name: "Percussion - Pitched" },
    Group { id: "unpitched-percussion", name: "Percussion - Unpitched" },
    Group { id: "marching-percussion", name: "Percussion - Marching" },
    Group { id: "body-percussion", name: "Percussion - Body" },
    Group { id: "vocals", name: "Vocals" },
    Group { id: "keyboards", name: "Keyboards" },
    Group { id: "electronic-instruments", name: "Electronic" },
    Group { id: "plucked-strings", name: "Strings - Plucked" },
    Group { id: "strings", name: "Strings - Bowed" },
];

pub static INSTRUMENTS: &[InstrumentDef] = &[
    InstrumentDef { id: "winds", name: "Winds", trait_name: "", group: 0, genres: 0, bank: 0, program: 73, drumset: false, bass_clef: false },
    InstrumentDef { id: "eb-piccolo", name: "Piccolo", trait_name: "Eb", group: 0, genres: 0, bank: 0, program: 72, drumset: false, bass_clef: false },
    InstrumentDef { id: "db-piccolo", name: "Piccolo", trait_name: "Db", group: 0, genres: 0, bank: 0, program: 72, drumset: false, bass_clef: false },
    InstrumentDef { id: "piccolo", name: "Piccolo", trait_name: "(C)", group: 0, genres: 117, bank: 0, program: 72, drumset: false, bass_clef: false },
    InstrumentDef { id: "treble-flute", name: "Treble Flute", trait_name: "", group: 0, genres: 0, bank: 0, program: 72, drumset: false, bass_clef: false },
    InstrumentDef { id: "danso", name: "Danso", trait_name: "", group: 0, genres: 256, bank: 0, program: 76, drumset: false, bass_clef: false },
    InstrumentDef { id: "daegeum", name: "Daegeum", trait_name: "", group: 0, genres: 256, bank: 0, program: 72, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-flute", name: "Soprano Flute", trait_name: "", group: 0, genres: 0, bank: 0, program: 73, drumset: false, bass_clef: false },
    InstrumentDef { id: "irish-flute", name: "Irish Flute", trait_name: "", group: 0, genres: 256, bank: 0, program: 73, drumset: false, bass_clef: false },
    InstrumentDef { id: "traverso", name: "Traverso", trait_name: "", group: 0, genres: 512, bank: 0, program: 73, drumset: false, bass_clef: false },
    InstrumentDef { id: "flute", name: "Flute", trait_name: "", group: 0, genres: 119, bank: 0, program: 73, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-flute", name: "Alto Flute", trait_name: "", group: 0, genres: 52, bank: 0, program: 73, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-flute", name: "Bass Flute", trait_name: "", group: 0, genres: 0, bank: 0, program: 73, drumset: false, bass_clef: false },
    InstrumentDef { id: "contra-alto-flute", name: "Contra-alto Flute", trait_name: "", group: 0, genres: 0, bank: 0, program: 73, drumset: false, bass_clef: false },
    InstrumentDef { id: "contrabass-flute", name: "Contrabass Flute", trait_name: "", group: 0, genres: 0, bank: 0, program: 73, drumset: false, bass_clef: false },
    InstrumentDef { id: "subcontra-alto-flute", name: "Sub Contra-alto Flute", trait_name: "", group: 0, genres: 0, bank: 0, program: 73, drumset: false, bass_clef: false },
    InstrumentDef { id: "double-contrabass-flute", name: "Double Contrabass Flute", trait_name: "", group: 0, genres: 0, bank: 0, program: 73, drumset: false, bass_clef: false },
    InstrumentDef { id: "hyperbass-flute", name: "Hyperbass Flute", trait_name: "", group: 0, genres: 0, bank: 0, program: 73, drumset: false, bass_clef: false },
    InstrumentDef { id: "a-dizi", name: "Dizi", trait_name: "A", group: 0, genres: 256, bank: 0, program: 72, drumset: false, bass_clef: false },
    InstrumentDef { id: "g-dizi", name: "Dizi", trait_name: "G", group: 0, genres: 256, bank: 0, program: 72, drumset: false, bass_clef: false },
    InstrumentDef { id: "f-dizi", name: "Dizi", trait_name: "F", group: 0, genres: 256, bank: 0, program: 72, drumset: false, bass_clef: false },
    InstrumentDef { id: "e-dizi", name: "Dizi", trait_name: "E", group: 0, genres: 256, bank: 0, program: 72, drumset: false, bass_clef: false },
    InstrumentDef { id: "d-dizi", name: "Dizi", trait_name: "D", group: 0, genres: 256, bank: 0, program: 72, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-dizi", name: "Dizi", trait_name: "C", group: 0, genres: 256, bank: 0, program: 72, drumset: false, bass_clef: false },
    InstrumentDef { id: "shakuhachi", name: "Shakuhachi", trait_name: "", group: 0, genres: 256, bank: 0, program: 77, drumset: false, bass_clef: false },
    InstrumentDef { id: "hichiriki", name: "Hichiriki", trait_name: "", group: 0, genres: 256, bank: 0, program: 68, drumset: false, bass_clef: false },
    InstrumentDef { id: "fife", name: "Fife", trait_name: "Bb", group: 0, genres: 0, bank: 0, program: 72, drumset: false, bass_clef: false },
    InstrumentDef { id: "d-tin-whistle", name: "Tin Whistle", trait_name: "D", group: 0, genres: 0, bank: 0, program: 78, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-tin-whistle", name: "Tin Whistle", trait_name: "C", group: 0, genres: 1024, bank: 0, program: 78, drumset: false, bass_clef: false },
    InstrumentDef { id: "bflat-tin-whistle", name: "Tin Whistle", trait_name: "Bb", group: 0, genres: 0, bank: 0, program: 78, drumset: false, bass_clef: false },
    InstrumentDef { id: "slide-whistle", name: "Slide Whistle", trait_name: "", group: 0, genres: 0, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "french-flageolet", name: "French Flageolet", trait_name: "", group: 0, genres: 0, bank: 0, program: 78, drumset: false, bass_clef: false },
    InstrumentDef { id: "english-flageolet", name: "English Flageolet", trait_name: "", group: 0, genres: 0, bank: 0, program: 78, drumset: false, bass_clef: false },
    InstrumentDef { id: "flageolet", name: "flageolet", trait_name: "", group: 0, genres: 0, bank: 0, program: 78, drumset: false, bass_clef: false },
    InstrumentDef { id: "garklein-recorder", name: "Garklein Recorder", trait_name: "", group: 0, genres: 512, bank: 0, program: 74, drumset: false, bass_clef: false },
    InstrumentDef { id: "sopranino-recorder", name: "Sopranino Recorder", trait_name: "", group: 0, genres: 512, bank: 0, program: 74, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-recorder", name: "Soprano Recorder", trait_name: "", group: 0, genres: 512, bank: 0, program: 74, drumset: false, bass_clef: false },
    InstrumentDef { id: "recorder", name: "recorder", trait_name: "", group: 0, genres: 1025, bank: 0, program: 74, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-recorder", name: "Alto Recorder", trait_name: "", group: 0, genres: 512, bank: 0, program: 74, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-recorder", name: "Tenor Recorder", trait_name: "", group: 0, genres: 512, bank: 0, program: 74, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-recorder", name: "Bass Recorder", trait_name: "", group: 0, genres: 512, bank: 0, program: 74, drumset: false, bass_clef: false },
    InstrumentDef { id: "greatbass-recorder", name: "Greatbass Recorder", trait_name: "", group: 0, genres: 512, bank: 0, program: 74, drumset: false, bass_clef: false },
    InstrumentDef { id: "contrabass-recorder", name: "Contrabass Recorder", trait_name: "", group: 0, genres: 512, bank: 0, program: 74, drumset: false, bass_clef: true },
    InstrumentDef { id: "c-soprano-ocarina", name: "Soprano Ocarina", trait_name: "C", group: 0, genres: 256, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "ocarina", name: "ocarina", trait_name: "", group: 0, genres: 256, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-soprano-ocarina", name: "Soprano Ocarina", trait_name: "Bb", group: 0, genres: 256, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "g-soprano-ocarina", name: "Soprano Ocarina", trait_name: "G", group: 0, genres: 256, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "f-soprano-ocarina", name: "Soprano Ocarina", trait_name: "F", group: 0, genres: 256, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-alto-ocarina", name: "Alto Ocarina", trait_name: "C", group: 0, genres: 256, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-alto-ocarina", name: "Alto Ocarina", trait_name: "Bb", group: 0, genres: 256, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "g-alto-ocarina", name: "Alto Ocarina", trait_name: "G", group: 0, genres: 256, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "f-alto-ocarina", name: "Alto Ocarina", trait_name: "F", group: 0, genres: 256, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-bass-ocarina", name: "Bass Ocarina", trait_name: "C", group: 0, genres: 256, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "gemshorn", name: "gemshorn", trait_name: "", group: 0, genres: 512, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-gemshorn", name: "Soprano Gemshorn", trait_name: "", group: 0, genres: 512, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-gemshorn", name: "Alto Gemshorn", trait_name: "", group: 0, genres: 512, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-gemshorn", name: "Tenor Gemshorn", trait_name: "", group: 0, genres: 512, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-gemshorn", name: "Bass Gemshorn", trait_name: "", group: 0, genres: 512, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "pan-flute", name: "Pan Flute", trait_name: "", group: 0, genres: 256, bank: 0, program: 75, drumset: false, bass_clef: false },
    InstrumentDef { id: "quena", name: "quena", trait_name: "", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-quena", name: "Quena", trait_name: "C", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "g-quena", name: "Quena", trait_name: "G", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "f-quena", name: "Quena", trait_name: "F", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "d-quena", name: "Quena", trait_name: "D", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "fs-castilian-dulzaina", name: "Castilian Dulzaina", trait_name: "F#", group: 0, genres: 256, bank: 0, program: 68, drumset: false, bass_clef: false },
    InstrumentDef { id: "piccolo-heckelphone", name: "Piccolo Heckelphone", trait_name: "", group: 0, genres: 0, bank: 0, program: 68, drumset: false, bass_clef: false },
    InstrumentDef { id: "piccolo-oboe", name: "Piccolo Oboe", trait_name: "", group: 0, genres: 0, bank: 0, program: 68, drumset: false, bass_clef: false },
    InstrumentDef { id: "baroque-oboe", name: "Baroque Oboe", trait_name: "", group: 0, genres: 512, bank: 0, program: 69, drumset: false, bass_clef: false },
    InstrumentDef { id: "oboe", name: "Oboe", trait_name: "", group: 0, genres: 49, bank: 0, program: 68, drumset: false, bass_clef: false },
    InstrumentDef { id: "oboe-d'amore", name: "Oboe d'amore", trait_name: "", group: 0, genres: 0, bank: 0, program: 68, drumset: false, bass_clef: false },
    InstrumentDef { id: "oboe-da-caccia", name: "Oboe da caccia", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "english-horn", name: "English Horn", trait_name: "", group: 0, genres: 48, bank: 0, program: 69, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-oboe", name: "Bass Oboe", trait_name: "", group: 0, genres: 0, bank: 0, program: 68, drumset: false, bass_clef: false },
    InstrumentDef { id: "heckelphone", name: "Heckelphone", trait_name: "", group: 0, genres: 0, bank: 0, program: 68, drumset: false, bass_clef: false },
    InstrumentDef { id: "lupophone", name: "Lupophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 68, drumset: false, bass_clef: false },
    InstrumentDef { id: "sopranino-shawm", name: "Sopranino Shawm", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-shawm", name: "Soprano Shawm", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-shawm", name: "Alto Shawm", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-shawm", name: "Tenor Shawm", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-shawm", name: "Bass Shawm", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: true },
    InstrumentDef { id: "great-bass-shawm", name: "Great Bass Shawm", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "cromorne", name: "Cromorne", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "crumhorn", name: "crumhorn", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-crumhorn", name: "Soprano Crumhorn", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-crumhorn", name: "Alto Crumhorn", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-crumhorn", name: "Tenor Crumhorn", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-crumhorn", name: "Bass Crumhorn", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: true },
    InstrumentDef { id: "greatbass-crumhorn", name: "Greatbass Crumhorn", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: true },
    InstrumentDef { id: "cornamuse", name: "cornamuse", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-cornamuse", name: "Soprano Cornamuse", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-cornamuse", name: "Alto Cornamuse", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-cornamuse", name: "Tenor Cornamuse", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-cornamuse", name: "Bass Cornamuse", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: true },
    InstrumentDef { id: "soprano-kelhorn", name: "Soprano Kelhorn", trait_name: "", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-kelhorn", name: "Alto Kelhorn", trait_name: "", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-kelhorn", name: "Tenor Kelhorn", trait_name: "", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-kelhorn", name: "Bass Kelhorn", trait_name: "", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: true },
    InstrumentDef { id: "greatbass-kelhorn", name: "Greatbass Kelhorn", trait_name: "", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: true },
    InstrumentDef { id: "sopranino-rauschpfeife", name: "Sopranino Rauschpfeife", trait_name: "", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "rauschpfeife", name: "rauschpfeife", trait_name: "", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-rauschpfeife", name: "Soprano Rauschpfeife", trait_name: "", group: 0, genres: 256, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "f-duduk", name: "Duduk", trait_name: "F", group: 0, genres: 256, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "e-duduk", name: "Duduk", trait_name: "E", group: 0, genres: 256, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "d-duduk", name: "Duduk", trait_name: "D", group: 0, genres: 256, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-duduk", name: "Duduk", trait_name: "C", group: 0, genres: 256, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "b-duduk", name: "Duduk", trait_name: "B", group: 0, genres: 256, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-duduk", name: "Duduk", trait_name: "Bb", group: 0, genres: 256, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "duduk", name: "duduk", trait_name: "", group: 0, genres: 256, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "a-duduk", name: "Duduk", trait_name: "A", group: 0, genres: 256, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "g-duduk", name: "Duduk", trait_name: "G", group: 0, genres: 256, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "a-bass-duduk", name: "Bass Duduk", trait_name: "A", group: 0, genres: 256, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "shenai", name: "Shenai", trait_name: "", group: 0, genres: 256, bank: 0, program: 111, drumset: false, bass_clef: false },
    InstrumentDef { id: "piccolo-clarinet", name: "Piccolo Clarinet", trait_name: "", group: 0, genres: 0, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-clarinet", name: "Soprano Clarinet", trait_name: "", group: 0, genres: 32, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "eb-clarinet", name: "Clarinet", trait_name: "Eb", group: 0, genres: 52, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "d-clarinet", name: "Clarinet", trait_name: "D", group: 0, genres: 0, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-clarinet", name: "Clarinet", trait_name: "C", group: 0, genres: 0, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-clarinet", name: "Clarinet", trait_name: "Bb", group: 0, genres: 117, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "clarinet", name: "clarinet", trait_name: "", group: 0, genres: 0, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "a-clarinet", name: "Clarinet", trait_name: "A", group: 0, genres: 16, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "g-clarinet", name: "Clarinet", trait_name: "G", group: 0, genres: 0, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "basset-clarinet", name: "Basset Clarinet", trait_name: "", group: 0, genres: 0, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-clarinet", name: "Alto Clarinet", trait_name: "", group: 0, genres: 32, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "basset-horn", name: "Basset Horn", trait_name: "", group: 0, genres: 16, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-clarinet", name: "bass-clarinet", trait_name: "", group: 0, genres: 0, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-bass-clarinet", name: "Bass Clarinet", trait_name: "(Bb)", group: 0, genres: 117, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-bass-clarinet-bass-clef", name: "Bass Clarinet (bass clef)", trait_name: "(Bb)", group: 0, genres: 0, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "a-bass-clarinet", name: "Bass Clarinet", trait_name: "A", group: 0, genres: 16, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "a-bass-clarinet-bass-clef)", name: "Bass Clarinet (bass clef)", trait_name: "A", group: 0, genres: 0, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "contra-alto-clarinet", name: "Contra-alto Clarinet", trait_name: "", group: 0, genres: 32, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "contrabass-clarinet", name: "Contrabass Clarinet", trait_name: "", group: 0, genres: 52, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "sopranino-chalumeau", name: "Sopranino Chalumeau", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-chalumeau", name: "Soprano Chalumeau", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-chalumeau", name: "Alto Chalumeau", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "chalumeau", name: "chalumeau", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-chalumeau", name: "Tenor Chalumeau", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-chalumeau", name: "Bass Chalumeau", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: true },
    InstrumentDef { id: "d-xaphoon", name: "Pocket Sax", trait_name: "D", group: 0, genres: 260, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "xaphoon", name: "Pocket Sax", trait_name: "C", group: 0, genres: 260, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-xaphoon", name: "Pocket Sax", trait_name: "Bb", group: 0, genres: 260, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "g-xaphoon", name: "Pocket Sax", trait_name: "G", group: 0, genres: 260, bank: 0, program: 71, drumset: false, bass_clef: false },
    InstrumentDef { id: "tarogato", name: "Tarogato", trait_name: "", group: 0, genres: 256, bank: 0, program: 69, drumset: false, bass_clef: false },
    InstrumentDef { id: "octavin", name: "Octavin", trait_name: "", group: 0, genres: 0, bank: 0, program: 69, drumset: false, bass_clef: false },
    InstrumentDef { id: "sopranissimo-saxophone", name: "Sopranissimo Saxophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 64, drumset: false, bass_clef: false },
    InstrumentDef { id: "sopranino-saxophone", name: "Sopranino Saxophone", trait_name: "", group: 0, genres: 4, bank: 0, program: 64, drumset: false, bass_clef: false },
    InstrumentDef { id: "aulochrome", name: "Aulochrome", trait_name: "", group: 0, genres: 0, bank: 0, program: 64, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-saxophone", name: "Soprano Saxophone", trait_name: "", group: 0, genres: 117, bank: 0, program: 64, drumset: false, bass_clef: false },
    InstrumentDef { id: "mezzo-soprano-saxophone", name: "Mezzo-Soprano Saxophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 65, drumset: false, bass_clef: false },
    InstrumentDef { id: "heckelphone-clarinet", name: "Heckelphone-clarinet", trait_name: "", group: 0, genres: 0, bank: 0, program: 68, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-saxophone", name: "Alto Saxophone", trait_name: "", group: 0, genres: 119, bank: 0, program: 65, drumset: false, bass_clef: false },
    InstrumentDef { id: "melody-saxophone", name: "Melody Saxophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 66, drumset: false, bass_clef: false },
    InstrumentDef { id: "saxophone", name: "saxophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 66, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-saxophone", name: "Tenor Saxophone", trait_name: "", group: 0, genres: 119, bank: 0, program: 66, drumset: false, bass_clef: false },
    InstrumentDef { id: "baritone-saxophone", name: "Baritone Saxophone", trait_name: "", group: 0, genres: 117, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-saxophone", name: "Bass Saxophone", trait_name: "", group: 0, genres: 4, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "contrabass-saxophone", name: "Contrabass Saxophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "subcontrabass-saxophone", name: "Subcontrabass Saxophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "bassoon", name: "Bassoon", trait_name: "", group: 0, genres: 49, bank: 0, program: 70, drumset: false, bass_clef: true },
    InstrumentDef { id: "contrabassoon", name: "Contrabassoon", trait_name: "", group: 0, genres: 48, bank: 0, program: 70, drumset: false, bass_clef: false },
    InstrumentDef { id: "reed-contrabass", name: "Reed Contrabass", trait_name: "", group: 0, genres: 0, bank: 0, program: 70, drumset: false, bass_clef: true },
    InstrumentDef { id: "dulcian", name: "Dulcian", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: true },
    InstrumentDef { id: "rackett", name: "Rackett", trait_name: "", group: 0, genres: 512, bank: 0, program: 67, drumset: false, bass_clef: true },
    InstrumentDef { id: "sopranino-sarrusophone", name: "Sopranino Sarrusophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "sarrusophone", name: "sarrusophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-sarrusophone", name: "Soprano Sarrusophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-sarrusophone", name: "Alto Sarrusophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-sarrusophone", name: "Tenor Sarrusophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "baritone-sarrusophone", name: "Baritone Sarrusophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-sarrusophone", name: "Bass Sarrusophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "contrabass-sarrusophone", name: "Contrabass Sarrusophone", trait_name: "", group: 0, genres: 0, bank: 0, program: 67, drumset: false, bass_clef: false },
    InstrumentDef { id: "bagpipe", name: "Bagpipe", trait_name: "", group: 0, genres: 256, bank: 0, program: 109, drumset: false, bass_clef: false },
    InstrumentDef { id: "accordion", name: "Accordion", trait_name: "", group: 1, genres: 5, bank: 0, program: 21, drumset: false, bass_clef: false },
    InstrumentDef { id: "bandoneon", name: "Bandoneon", trait_name: "", group: 1, genres: 256, bank: 0, program: 23, drumset: false, bass_clef: false },
    InstrumentDef { id: "concertina", name: "Concertina", trait_name: "", group: 1, genres: 0, bank: 0, program: 21, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica", name: "harmonica", trait_name: "", group: 1, genres: 7, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-d10high-g", name: "10-Hole Diatonic Harmonica", trait_name: "High G", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-d10f", name: "10-Hole Diatonic Harmonica", trait_name: "F", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-d10d", name: "10-Hole Diatonic Harmonica", trait_name: "D", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-d10c", name: "10-Hole Diatonic Harmonica", trait_name: "C", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-d10a", name: "10-Hole Diatonic Harmonica", trait_name: "A", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-d10g", name: "10-Hole Diatonic Harmonica", trait_name: "G", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-d10low-d", name: "10-Hole Diatonic Harmonica", trait_name: "Low D", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-c12c", name: "12-Hole Chromatic Harmonica", trait_name: "C", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-c12g", name: "12-Hole Chromatic Harmonica", trait_name: "G", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-c12tenor-c", name: "12-Hole Chromatic Harmonica", trait_name: "Tenor C", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-c14c", name: "14-Hole Chromatic Harmonica", trait_name: "C", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-c16c", name: "16-Hole Chromatic Harmonica", trait_name: "C", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonica-chordet", name: "20-Hole Chordet Harmonica", trait_name: "", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-harmonica-hohner", name: "Bass Hohner Harmonica", trait_name: "", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: true },
    InstrumentDef { id: "bass-harmonica-huang", name: "Bass Huang Harmonica", trait_name: "", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: true },
    InstrumentDef { id: "bass-harmonica", name: "Bass Harmonica", trait_name: "", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: true },
    InstrumentDef { id: "melodica", name: "Melodica", trait_name: "", group: 1, genres: 0, bank: 0, program: 22, drumset: false, bass_clef: false },
    InstrumentDef { id: "sho", name: "Sho", trait_name: "", group: 1, genres: 256, bank: 0, program: 20, drumset: false, bass_clef: false },
    InstrumentDef { id: "sheng", name: "sheng", trait_name: "", group: 1, genres: 256, bank: 0, program: 20, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-sheng", name: "Soprano Sheng", trait_name: "", group: 1, genres: 256, bank: 0, program: 20, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-sheng", name: "Alto Sheng", trait_name: "", group: 1, genres: 256, bank: 0, program: 20, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-sheng", name: "Tenor Sheng", trait_name: "", group: 1, genres: 256, bank: 0, program: 20, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-sheng", name: "Bass Sheng", trait_name: "", group: 1, genres: 256, bank: 0, program: 20, drumset: false, bass_clef: true },
    InstrumentDef { id: "brass", name: "Brass", trait_name: "", group: 2, genres: 0, bank: 0, program: 61, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-horn-alto", name: "Horn", trait_name: "C alto", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-horn-alto", name: "Horn", trait_name: "Bb alto", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "a-horn", name: "Horn", trait_name: "A", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "ab-horn", name: "Horn", trait_name: "Ab", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "g-horn", name: "Horn", trait_name: "G", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "e-horn", name: "Horn", trait_name: "E", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "eb-horn", name: "Horn", trait_name: "Eb", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "d-horn", name: "Horn", trait_name: "D", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "horn", name: "Horn", trait_name: "F", group: 2, genres: 113, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-horn", name: "Horn", trait_name: "C", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-horn-bass", name: "Horn (bass clef)", trait_name: "C", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: true },
    InstrumentDef { id: "bb-horn-basso", name: "Horn", trait_name: "Bb basso", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "vienna-horn", name: "Vienna Horn", trait_name: "", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-wagner-tuba", name: "Wagner Tuba", trait_name: "Bb", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: true },
    InstrumentDef { id: "f-wagner-tuba", name: "Wagner Tuba", trait_name: "F", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: true },
    InstrumentDef { id: "wagner-tuba", name: "wagner-tuba", trait_name: "", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: true },
    InstrumentDef { id: "eb-cornet", name: "Cornet", trait_name: "Eb", group: 2, genres: 32, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-cornet", name: "Cornet", trait_name: "C", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-cornet", name: "Cornet", trait_name: "Bb", group: 2, genres: 37, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "a-cornet", name: "Cornet", trait_name: "A", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "saxhorn", name: "Saxhorn", trait_name: "", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "f-alto-horn", name: "Alto Horn", trait_name: "F", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "eb-alto-horn", name: "Alto Horn", trait_name: "Eb", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "baritone-horn", name: "Baritone Horn", trait_name: "", group: 2, genres: 96, bank: 0, program: 60, drumset: false, bass_clef: true },
    InstrumentDef { id: "baritone-horn-treble", name: "Baritone Horn (treble clef)", trait_name: "", group: 2, genres: 32, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "baritone-horn-central-europe-treble", name: "Baritone Horn (Central European, treble clef)", trait_name: "", group: 2, genres: 96, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "baritone-horn-central-europe", name: "Baritone Horn (Central European)", trait_name: "", group: 2, genres: 96, bank: 0, program: 60, drumset: false, bass_clef: true },
    InstrumentDef { id: "posthorn", name: "Posthorn", trait_name: "", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: true },
    InstrumentDef { id: "bb-piccolo-trumpet", name: "Piccolo Trumpet", trait_name: "Bb", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "piccolo-trumpet", name: "piccolo-trumpet", trait_name: "", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "a-piccolo-trumpet", name: "Piccolo Trumpet", trait_name: "A", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "f-trumpet", name: "Trumpet", trait_name: "F", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "e-trumpet", name: "Trumpet", trait_name: "E", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "eb-trumpet", name: "Trumpet", trait_name: "Eb", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "d-trumpet", name: "Trumpet", trait_name: "D", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-trumpet", name: "Trumpet", trait_name: "C", group: 2, genres: 16, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "trumpet", name: "trumpet", trait_name: "", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-trumpet", name: "Trumpet", trait_name: "Bb", group: 2, genres: 119, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "a-trumpet", name: "Trumpet", trait_name: "A", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "pocket-trumpet", name: "Pocket Trumpet", trait_name: "", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "slide-trumpet", name: "Slide Trumpet", trait_name: "", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-trumpet", name: "Tenor Trumpet", trait_name: "", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "eb-bass-trumpet", name: "Bass Trumpet", trait_name: "Eb", group: 2, genres: 16, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-bass-trumpet", name: "Bass Trumpet", trait_name: "C", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-trumpet", name: "bass-trumpet", trait_name: "", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-bass-trumpet", name: "Bass Trumpet", trait_name: "Bb", group: 2, genres: 16, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "f-baroque-trumpet", name: "Baroque Trumpet", trait_name: "F", group: 2, genres: 512, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "eb-baroque-trumpet", name: "Baroque Trumpet", trait_name: "Eb", group: 2, genres: 512, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "d-baroque-trumpet", name: "Baroque Trumpet", trait_name: "D", group: 2, genres: 512, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-baroque-trumpet", name: "Baroque Trumpet", trait_name: "C", group: 2, genres: 512, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "bb-baroque-trumpet", name: "Baroque Trumpet", trait_name: "Bb", group: 2, genres: 512, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "baroque-trumpet", name: "baroque-trumpet", trait_name: "", group: 2, genres: 512, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "bugle", name: "Bugle", trait_name: "", group: 2, genres: 0, bank: 0, program: 58, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-bugle", name: "Soprano Bugle", trait_name: "", group: 2, genres: 64, bank: 0, program: 58, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-bugle", name: "Alto Bugle", trait_name: "", group: 2, genres: 64, bank: 0, program: 58, drumset: false, bass_clef: false },
    InstrumentDef { id: "mellophone", name: "Mellophone", trait_name: "", group: 2, genres: 68, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "baritone-bugle", name: "Baritone Bugle", trait_name: "", group: 2, genres: 64, bank: 0, program: 58, drumset: false, bass_clef: false },
    InstrumentDef { id: "euphonium-bugle", name: "Euphonium Bugle", trait_name: "", group: 2, genres: 64, bank: 0, program: 58, drumset: false, bass_clef: false },
    InstrumentDef { id: "mellophon-bugle", name: "Mellophone Bugle", trait_name: "", group: 2, genres: 64, bank: 0, program: 58, drumset: false, bass_clef: false },
    InstrumentDef { id: "contrabass-bugle", name: "Contrabass Bugle", trait_name: "", group: 2, genres: 64, bank: 0, program: 58, drumset: false, bass_clef: false },
    InstrumentDef { id: "fiscorn", name: "Fiscorn", trait_name: "", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "flugelhorn", name: "Flugelhorn", trait_name: "", group: 2, genres: 100, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "kuhlohorn", name: "Kuhlohorn", trait_name: "", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "f-alto-ophicleide", name: "Alto Ophicleide", trait_name: "F", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "eb-alto-ophicleide", name: "Alto Ophicleide", trait_name: "Eb", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "ophicleide", name: "ophicleide", trait_name: "", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: true },
    InstrumentDef { id: "c-bass-ophicleide", name: "Bass Ophicleide", trait_name: "C", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: true },
    InstrumentDef { id: "bb-bass-ophicleide", name: "Bass Ophicleide", trait_name: "Bb", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: true },
    InstrumentDef { id: "eb-contrabass-ophicleide", name: "Contrabass Ophicleide", trait_name: "Eb", group: 2, genres: 0, bank: 0, program: 56, drumset: false, bass_clef: true },
    InstrumentDef { id: "cornettino", name: "Cornettino", trait_name: "", group: 2, genres: 512, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "cornett", name: "cornett", trait_name: "", group: 2, genres: 512, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-cornett", name: "Soprano Cornett", trait_name: "", group: 2, genres: 512, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-cornett", name: "Alto Cornett", trait_name: "", group: 2, genres: 512, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-cornett", name: "Tenor Cornett", trait_name: "", group: 2, genres: 512, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "serpent", name: "Serpent", trait_name: "", group: 2, genres: 512, bank: 0, program: 56, drumset: false, bass_clef: true },
    InstrumentDef { id: "soprano-trombone", name: "Soprano Trombone", trait_name: "", group: 2, genres: 0, bank: 0, program: 57, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-trombone", name: "Alto Trombone", trait_name: "", group: 2, genres: 16, bank: 0, program: 57, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-trombone", name: "Tenor Trombone", trait_name: "", group: 2, genres: 0, bank: 0, program: 57, drumset: false, bass_clef: true },
    InstrumentDef { id: "trombone", name: "Trombone", trait_name: "", group: 2, genres: 119, bank: 0, program: 57, drumset: false, bass_clef: true },
    InstrumentDef { id: "trombone-treble", name: "Trombone (treble clef)", trait_name: "", group: 2, genres: 0, bank: 0, program: 57, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-trombone", name: "Bass Trombone", trait_name: "", group: 2, genres: 116, bank: 0, program: 57, drumset: false, bass_clef: true },
    InstrumentDef { id: "contrabass-trombone", name: "Contrabass Trombone", trait_name: "", group: 2, genres: 16, bank: 0, program: 57, drumset: false, bass_clef: true },
    InstrumentDef { id: "cimbasso", name: "Cimbasso", trait_name: "", group: 2, genres: 0, bank: 0, program: 57, drumset: false, bass_clef: true },
    InstrumentDef { id: "alto-sackbut", name: "Alto Sackbut", trait_name: "", group: 2, genres: 512, bank: 0, program: 57, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-sackbut", name: "Tenor Sackbut", trait_name: "", group: 2, genres: 512, bank: 0, program: 57, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-sackbut", name: "Bass Sackbut", trait_name: "", group: 2, genres: 512, bank: 0, program: 57, drumset: false, bass_clef: true },
    InstrumentDef { id: "euphonium", name: "Euphonium", trait_name: "", group: 2, genres: 96, bank: 0, program: 58, drumset: false, bass_clef: true },
    InstrumentDef { id: "euphonium-treble", name: "Euphonium (treble clef)", trait_name: "", group: 2, genres: 96, bank: 0, program: 58, drumset: false, bass_clef: false },
    InstrumentDef { id: "tuba", name: "Tuba (unspecified)", trait_name: "", group: 2, genres: 117, bank: 0, program: 58, drumset: false, bass_clef: true },
    InstrumentDef { id: "f-tuba", name: "f-tuba", trait_name: "", group: 2, genres: 16, bank: 0, program: 58, drumset: false, bass_clef: true },
    InstrumentDef { id: "eb-tuba", name: "eb-tuba", trait_name: "", group: 2, genres: 0, bank: 0, program: 58, drumset: false, bass_clef: true },
    InstrumentDef { id: "bass-eb-tuba", name: "Bass Tuba", trait_name: "Eb", group: 2, genres: 0, bank: 0, program: 58, drumset: false, bass_clef: true },
    InstrumentDef { id: "bass-f-tuba", name: "Bass Tuba", trait_name: "F", group: 2, genres: 0, bank: 0, program: 58, drumset: false, bass_clef: true },
    InstrumentDef { id: "eb-tuba-treble", name: "Bass Tuba (treble clef)", trait_name: "Eb", group: 2, genres: 0, bank: 0, program: 58, drumset: false, bass_clef: false },
    InstrumentDef { id: "c-tuba", name: "Contrabass Tuba", trait_name: "C", group: 2, genres: 0, bank: 0, program: 58, drumset: false, bass_clef: true },
    InstrumentDef { id: "bb-tuba", name: "Contrabass Tuba", trait_name: "Bb", group: 2, genres: 16, bank: 0, program: 58, drumset: false, bass_clef: true },
    InstrumentDef { id: "bb-tuba-treble", name: "Contrabass Tuba (treble clef)", trait_name: "Bb", group: 2, genres: 0, bank: 0, program: 58, drumset: false, bass_clef: false },
    InstrumentDef { id: "subcontrabass-tuba", name: "Sub-Contrabass Tuba", trait_name: "", group: 2, genres: 0, bank: 0, program: 58, drumset: false, bass_clef: true },
    InstrumentDef { id: "bb-sousaphone", name: "Sousaphone", trait_name: "(Bb)", group: 2, genres: 68, bank: 0, program: 58, drumset: false, bass_clef: true },
    InstrumentDef { id: "bb-sousaphone-treble", name: "Sousaphone (treble clef)", trait_name: "(Bb)", group: 2, genres: 68, bank: 0, program: 58, drumset: false, bass_clef: false },
    InstrumentDef { id: "sousaphone", name: "Sousaphone (concert pitch)", trait_name: "", group: 2, genres: 68, bank: 0, program: 58, drumset: false, bass_clef: true },
    InstrumentDef { id: "helicon", name: "Helicon", trait_name: "", group: 2, genres: 0, bank: 0, program: 58, drumset: false, bass_clef: true },
    InstrumentDef { id: "conch", name: "Conch", trait_name: "", group: 2, genres: 256, bank: 0, program: 60, drumset: false, bass_clef: true },
    InstrumentDef { id: "horagai", name: "Horagai", trait_name: "", group: 2, genres: 256, bank: 0, program: 60, drumset: false, bass_clef: true },
    InstrumentDef { id: "alphorn", name: "Alphorn", trait_name: "", group: 2, genres: 0, bank: 0, program: 60, drumset: false, bass_clef: true },
    InstrumentDef { id: "rag-dung", name: "Rag Dung", trait_name: "", group: 2, genres: 256, bank: 0, program: 56, drumset: false, bass_clef: true },
    InstrumentDef { id: "didgeridoo", name: "Didgeridoo", trait_name: "", group: 2, genres: 256, bank: 0, program: 57, drumset: false, bass_clef: true },
    InstrumentDef { id: "shofar", name: "Shofar", trait_name: "", group: 2, genres: 256, bank: 0, program: 60, drumset: false, bass_clef: false },
    InstrumentDef { id: "vuvuzela", name: "Vuvuzela", trait_name: "", group: 2, genres: 256, bank: 0, program: 56, drumset: false, bass_clef: false },
    InstrumentDef { id: "timpani", name: "Timpani", trait_name: "", group: 3, genres: 113, bank: 0, program: 47, drumset: false, bass_clef: true },
    InstrumentDef { id: "roto-toms", name: "Roto-Toms", trait_name: "", group: 3, genres: 0, bank: 0, program: 117, drumset: false, bass_clef: false },
    InstrumentDef { id: "tubaphone", name: "Tubaphone", trait_name: "", group: 3, genres: 0, bank: 0, program: 47, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-steel-drums", name: "Lead Steel Pan", trait_name: "", group: 3, genres: 256, bank: 0, program: 114, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-steel-drums", name: "Double Tenor Steel Pans", trait_name: "", group: 3, genres: 256, bank: 0, program: 114, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-steel-drums", name: "Double Second Steel Pans", trait_name: "", group: 3, genres: 256, bank: 0, program: 114, drumset: false, bass_clef: false },
    InstrumentDef { id: "guitar-steel-drums", name: "Guitar Steel Pans", trait_name: "", group: 3, genres: 256, bank: 0, program: 114, drumset: false, bass_clef: true },
    InstrumentDef { id: "cello-steel-drums", name: "Triple Cello Steel Pans", trait_name: "", group: 3, genres: 256, bank: 0, program: 114, drumset: false, bass_clef: true },
    InstrumentDef { id: "cello-steel-drums-four", name: "Four Cello Steel Pans", trait_name: "", group: 3, genres: 256, bank: 0, program: 114, drumset: false, bass_clef: true },
    InstrumentDef { id: "tenor-bass-steel-drums", name: "Tenor Bass Steel Pans", trait_name: "", group: 3, genres: 256, bank: 0, program: 114, drumset: false, bass_clef: true },
    InstrumentDef { id: "steel-drums", name: "Steel Pans", trait_name: "", group: 3, genres: 256, bank: 0, program: 114, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-steel-drums", name: "Bass Steel Pans", trait_name: "", group: 3, genres: 256, bank: 0, program: 114, drumset: false, bass_clef: true },
    InstrumentDef { id: "glockenspiel", name: "Glockenspiel", trait_name: "", group: 3, genres: 1141, bank: 0, program: 9, drumset: false, bass_clef: false },
    InstrumentDef { id: "xylophone", name: "Xylophone", trait_name: "", group: 3, genres: 1141, bank: 0, program: 13, drumset: false, bass_clef: false },
    InstrumentDef { id: "xylomarimba", name: "Xylomarimba", trait_name: "", group: 3, genres: 20, bank: 0, program: 12, drumset: false, bass_clef: false },
    InstrumentDef { id: "vibraphone", name: "Vibraphone", trait_name: "", group: 3, genres: 117, bank: 0, program: 11, drumset: false, bass_clef: false },
    InstrumentDef { id: "dulcimer", name: "Dulcimer", trait_name: "", group: 3, genres: 0, bank: 0, program: 15, drumset: false, bass_clef: false },
    InstrumentDef { id: "cimbalom", name: "Cimbalom", trait_name: "", group: 3, genres: 0, bank: 0, program: 15, drumset: false, bass_clef: false },
    InstrumentDef { id: "marimba", name: "Marimba", trait_name: "", group: 3, genres: 117, bank: 0, program: 12, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-marimba", name: "Bass Marimba", trait_name: "", group: 3, genres: 16, bank: 0, program: 12, drumset: false, bass_clef: true },
    InstrumentDef { id: "contrabass-marimba", name: "Contrabass Marimba", trait_name: "", group: 3, genres: 16, bank: 0, program: 12, drumset: false, bass_clef: true },
    InstrumentDef { id: "crotales", name: "Crotales", trait_name: "", group: 3, genres: 112, bank: 0, program: 10, drumset: false, bass_clef: false },
    InstrumentDef { id: "almglocken", name: "Almglocken", trait_name: "", group: 3, genres: 0, bank: 0, program: 108, drumset: false, bass_clef: false },
    InstrumentDef { id: "tubular-bells", name: "Chimes", trait_name: "", group: 3, genres: 49, bank: 0, program: 14, drumset: false, bass_clef: false },
    InstrumentDef { id: "carillon", name: "Carillon", trait_name: "", group: 3, genres: 256, bank: 0, program: 14, drumset: false, bass_clef: false },
    InstrumentDef { id: "tuned-gongs", name: "Tuned Gongs", trait_name: "", group: 3, genres: 0, bank: 0, program: 14, drumset: false, bass_clef: true },
    InstrumentDef { id: "hand-bells", name: "Handbells", trait_name: "", group: 3, genres: 9, bank: 0, program: 112, drumset: false, bass_clef: false },
    InstrumentDef { id: "hand-chimes", name: "Handchimes", trait_name: "", group: 3, genres: 9, bank: 0, program: 112, drumset: false, bass_clef: false },
    InstrumentDef { id: "orff-soprano-glockenspiel", name: "Orff Soprano Glockenspiel", trait_name: "", group: 3, genres: 1024, bank: 0, program: 9, drumset: false, bass_clef: false },
    InstrumentDef { id: "orff-alto-glockenspiel", name: "Orff Alto Glockenspiel", trait_name: "", group: 3, genres: 1024, bank: 0, program: 9, drumset: false, bass_clef: false },
    InstrumentDef { id: "orff-soprano-metallophone", name: "Orff Soprano Metallophone", trait_name: "", group: 3, genres: 1024, bank: 0, program: 11, drumset: false, bass_clef: false },
    InstrumentDef { id: "orff-soprano-xylophone", name: "Orff Soprano Xylophone", trait_name: "", group: 3, genres: 1024, bank: 0, program: 12, drumset: false, bass_clef: false },
    InstrumentDef { id: "metallophone", name: "metallophone", trait_name: "", group: 3, genres: 1024, bank: 0, program: 11, drumset: false, bass_clef: false },
    InstrumentDef { id: "orff-alto-metallophone", name: "Orff Alto Metallophone", trait_name: "", group: 3, genres: 1024, bank: 0, program: 11, drumset: false, bass_clef: false },
    InstrumentDef { id: "orff-alto-xylophone", name: "Orff Alto Xylophone", trait_name: "", group: 3, genres: 1024, bank: 0, program: 12, drumset: false, bass_clef: false },
    InstrumentDef { id: "orff-bass-metallophone", name: "Orff Bass Metallophone", trait_name: "", group: 3, genres: 1024, bank: 0, program: 11, drumset: false, bass_clef: false },
    InstrumentDef { id: "orff-bass-xylophone", name: "Orff Bass Xylophone", trait_name: "", group: 3, genres: 1024, bank: 0, program: 12, drumset: false, bass_clef: false },
    InstrumentDef { id: "flexatone", name: "Flexatone", trait_name: "", group: 3, genres: 0, bank: 0, program: 14, drumset: false, bass_clef: false },
    InstrumentDef { id: "musical-saw", name: "Musical Saw", trait_name: "", group: 3, genres: 0, bank: 0, program: 92, drumset: false, bass_clef: false },
    InstrumentDef { id: "musical-glasses", name: "Musical Glasses", trait_name: "", group: 3, genres: 0, bank: 0, program: 93, drumset: false, bass_clef: false },
    InstrumentDef { id: "glass-harmonica", name: "Glass Harmonica", trait_name: "", group: 3, genres: 0, bank: 0, program: 92, drumset: false, bass_clef: false },
    InstrumentDef { id: "tuned-klaxon-horns", name: "Tuned Klaxon Horns", trait_name: "", group: 3, genres: 0, bank: 0, program: 84, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-kalimba", name: "Alto Kalimba", trait_name: "", group: 3, genres: 256, bank: 0, program: 108, drumset: false, bass_clef: false },
    InstrumentDef { id: "kalimba", name: "Kalimba", trait_name: "", group: 3, genres: 256, bank: 0, program: 108, drumset: false, bass_clef: false },
    InstrumentDef { id: "treble-kalimba", name: "Treble Kalimba", trait_name: "", group: 3, genres: 256, bank: 0, program: 108, drumset: false, bass_clef: false },
    InstrumentDef { id: "automobile-brake-drums", name: "Automobile Brake Drums", trait_name: "", group: 4, genres: 96, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "bongos", name: "Bongos", trait_name: "", group: 4, genres: 1142, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "janggu", name: "Janggu", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "samul-janggu", name: "Samul Janggu", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "sanjo-janggu", name: "Sanjo Janggu", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "ko-tsuzumi", name: "Ko-Tsuzumi", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "o-tsuzumi", name: "O-Tsuzumi", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "chinese-tom-toms", name: "Chinese Tom-Toms", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "sogo", name: "Sogo", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "bass-drum", name: "Bass Drum", trait_name: "", group: 4, genres: 113, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "okedo-daiko", name: "Okedo-Daiko", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "snare-drum", name: "Snare Drum", trait_name: "", group: 4, genres: 121, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "tom-toms", name: "Concert Toms", trait_name: "", group: 4, genres: 112, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "congas", name: "Congas", trait_name: "", group: 4, genres: 118, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "shime-daiko", name: "Shime-Daiko", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "kakko", name: "Kakko", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "xiaogu", name: "Xiaogu", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "doumbek", name: "Doumbek", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "djembe", name: "Djembe", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "cajon", name: "Cajon", trait_name: "", group: 4, genres: 256, bank: 128, program: 40, drumset: true, bass_clef: false },
    InstrumentDef { id: "cuica", name: "Cuica", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "drumset", name: "Drum Kit (large)", trait_name: "", group: 4, genres: 115, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "drum-kit-4", name: "Drum Kit (minimal)", trait_name: "", group: 4, genres: 115, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "drum-kit-5", name: "Drum Kit (common)", trait_name: "", group: 4, genres: 115, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "military-drum", name: "Field Drum", trait_name: "", group: 4, genres: 0, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "frame-drum", name: "Frame Drum", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "bangu", name: "Bangu", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "piccolo-snare-drum", name: "Piccolo Snare", trait_name: "", group: 4, genres: 0, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "slit-drum", name: "Slit Drum", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "tablas", name: "Tablas", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "timbales", name: "Timbales", trait_name: "", group: 4, genres: 100, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "buk", name: "Buk", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "samul-buk", name: "Samul Buk", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "sori-buk", name: "Sori Buk", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "taiko", name: "Taiko", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "tsuri-daiko", name: "Tsuri-Daiko", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "miya-daiko", name: "Miya-Daiko", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "dagu", name: "Dagu", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "anvil", name: "Anvil", trait_name: "", group: 4, genres: 16, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "shoko", name: "Shoko", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "kane", name: "Kane", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "bell-plate", name: "Bell Plate", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "bell-tree", name: "Bell Tree", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "bowl-gongs", name: "Bowl Gongs", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "chains", name: "Chains", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "china-cymbal", name: "China Cymbal", trait_name: "", group: 4, genres: 112, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "opera-gong", name: "Opera Gong", trait_name: "", group: 4, genres: 48, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "wind-gong", name: "Wind Gong", trait_name: "", group: 4, genres: 48, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "daluo", name: "Daluo", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "xiaoluo", name: "Xiaoluo", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "dabo", name: "Dabo", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "naobo", name: "Naobo", trait_name: "", group: 4, genres: 48, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "kkwaenggwari", name: "Kkwaenggwari", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "cowbell", name: "Cowbell", trait_name: "", group: 4, genres: 1024, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "agogo-bells", name: "Agogo Bells", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "crash-cymbal", name: "Suspended Cymbal", trait_name: "", group: 4, genres: 1137, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "cymbal", name: "Hand Cymbals", trait_name: "", group: 4, genres: 1137, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "finger-cymbals", name: "Finger Cymbals", trait_name: "", group: 4, genres: 1024, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "hi-hat", name: "Hi-hat", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "iron-pipes", name: "Iron Pipes", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "mark-tree", name: "Mark Tree", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "metal-castanets", name: "Metal Castanets", trait_name: "", group: 4, genres: 272, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "metal-wind-chimes", name: "Metal Wind Chimes", trait_name: "", group: 4, genres: 112, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "ride-cymbal", name: "Ride Cymbal", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "sleigh-bells", name: "Sleigh Bells", trait_name: "", group: 4, genres: 48, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "splash-cymbal", name: "Splash Cymbal", trait_name: "", group: 4, genres: 96, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "tam-tam", name: "Tam-tam", trait_name: "", group: 4, genres: 112, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "jing", name: "Jing", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "thundersheet", name: "Thundersheet", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "triangle", name: "Triangle", trait_name: "", group: 4, genres: 1137, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "bamboo-wind-chimes", name: "Bamboo Wind Chimes", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "castanets", name: "Castanets", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "claves", name: "Claves", trait_name: "", group: 4, genres: 1125, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "ban", name: "Ban", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "guiro", name: "Guiro", trait_name: "", group: 4, genres: 1284, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "temple-blocks", name: "Temple Blocks", trait_name: "", group: 4, genres: 48, bank: 1, program: 115, drumset: true, bass_clef: false },
    InstrumentDef { id: "log-drum", name: "Log Drum", trait_name: "", group: 4, genres: 48, bank: 1, program: 115, drumset: true, bass_clef: false },
    InstrumentDef { id: "wood-blocks", name: "Wood Blocks", trait_name: "", group: 4, genres: 1137, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "bangzi", name: "Bangzi", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "wooden-wind-chimes", name: "Wooden Wind Chimes", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "cabasa", name: "Cabasa", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "glass-wind-chimes", name: "Glass Wind Chimes", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "maracas", name: "Maracas", trait_name: "", group: 4, genres: 1296, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "quijada", name: "Quijada", trait_name: "", group: 4, genres: 256, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "ratchet", name: "Ratchet", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "sandpaper-blocks", name: "Sandpaper Blocks", trait_name: "", group: 4, genres: 1024, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "shaker", name: "Shaker", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "shekere", name: "Shekere", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "shell-wind-chimes", name: "Shell Wind Chimes", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "stones", name: "Stones", trait_name: "", group: 4, genres: 0, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "tambourine", name: "Tambourine", trait_name: "", group: 4, genres: 1139, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "tubo", name: "Tubo", trait_name: "", group: 4, genres: 256, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "vibraslap", name: "Vibraslap", trait_name: "", group: 4, genres: 1056, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "whip", name: "Whip", trait_name: "", group: 4, genres: 1072, bank: 128, program: 56, drumset: true, bass_clef: false },
    InstrumentDef { id: "cannon", name: "Cannon", trait_name: "", group: 4, genres: 48, bank: 0, program: 127, drumset: true, bass_clef: false },
    InstrumentDef { id: "bird-call", name: "Bird Call", trait_name: "", group: 4, genres: 48, bank: 0, program: 123, drumset: true, bass_clef: false },
    InstrumentDef { id: "ocean-drum", name: "Ocean Drum", trait_name: "", group: 4, genres: 48, bank: 0, program: 122, drumset: true, bass_clef: false },
    InstrumentDef { id: "percussion-synthesizer", name: "General MIDI Percussion", trait_name: "", group: 4, genres: 247, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "percussion", name: "Mixed Percussion", trait_name: "", group: 4, genres: 119, bank: 128, program: 48, drumset: true, bass_clef: false },
    InstrumentDef { id: "marching-snare", name: "Marching Snare Drum", trait_name: "", group: 5, genres: 64, bank: 128, program: 56, drumset: true, bass_clef: false },
    InstrumentDef { id: "marching-tenor-drums", name: "Marching Tenor Drums", trait_name: "", group: 5, genres: 64, bank: 128, program: 96, drumset: true, bass_clef: false },
    InstrumentDef { id: "marching-show-tenors", name: "Show-Style Tenors", trait_name: "", group: 5, genres: 64, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "marching-bass-drums", name: "Marching Bass Drums", trait_name: "", group: 5, genres: 64, bank: 128, program: 59, drumset: true, bass_clef: false },
    InstrumentDef { id: "marching-cymbals", name: "Marching Cymbals", trait_name: "", group: 5, genres: 64, bank: 128, program: 58, drumset: true, bass_clef: false },
    InstrumentDef { id: "finger-snap", name: "Finger Snap", trait_name: "", group: 6, genres: 1035, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "hand-clap", name: "Hand Clap", trait_name: "", group: 6, genres: 1035, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "slap", name: "Slap", trait_name: "", group: 6, genres: 1035, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "stamp", name: "Stamp", trait_name: "", group: 6, genres: 1035, bank: 128, program: 0, drumset: true, bass_clef: false },
    InstrumentDef { id: "boy-soprano", name: "Boy Soprano", trait_name: "", group: 7, genres: 24, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano", name: "Soprano", trait_name: "", group: 7, genres: 29, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-c-clef", name: "Soprano (C clef)", trait_name: "", group: 7, genres: 512, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "mezzo-soprano", name: "Mezzo-soprano", trait_name: "", group: 7, genres: 25, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "mezzo-soprano-c-clef", name: "Mezzo-soprano (C clef)", trait_name: "", group: 7, genres: 512, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "countertenor", name: "Countertenor", trait_name: "", group: 7, genres: 8, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto", name: "Alto", trait_name: "", group: 7, genres: 29, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-c-clef", name: "Alto (C clef)", trait_name: "", group: 7, genres: 512, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "contralto", name: "Contralto", trait_name: "", group: 7, genres: 24, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor", name: "Tenor", trait_name: "", group: 7, genres: 29, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-c-clef", name: "Tenor (C clef)", trait_name: "", group: 7, genres: 512, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "baritone", name: "Baritone", trait_name: "", group: 7, genres: 29, bank: 0, program: 52, drumset: false, bass_clef: true },
    InstrumentDef { id: "baritone-c-clef", name: "Baritone (C clef)", trait_name: "", group: 7, genres: 512, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "voice", name: "Voice", trait_name: "", group: 7, genres: 13, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass", name: "Bass", trait_name: "", group: 7, genres: 29, bank: 0, program: 52, drumset: false, bass_clef: true },
    InstrumentDef { id: "women", name: "Women", trait_name: "", group: 7, genres: 29, bank: 0, program: 52, drumset: false, bass_clef: false },
    InstrumentDef { id: "men", name: "Men", trait_name: "", group: 7, genres: 29, bank: 0, program: 52, drumset: false, bass_clef: true },
    InstrumentDef { id: "kazoo", name: "Kazoo", trait_name: "", group: 7, genres: 9, bank: 0, program: 85, drumset: false, bass_clef: false },
    InstrumentDef { id: "celesta", name: "Celesta", trait_name: "", group: 8, genres: 16, bank: 0, program: 8, drumset: false, bass_clef: false },
    InstrumentDef { id: "clavichord", name: "Clavichord", trait_name: "", group: 8, genres: 512, bank: 0, program: 7, drumset: false, bass_clef: false },
    InstrumentDef { id: "clavinet", name: "Clavinet", trait_name: "", group: 8, genres: 4, bank: 0, program: 7, drumset: false, bass_clef: false },
    InstrumentDef { id: "harpsichord", name: "Harpsichord", trait_name: "", group: 8, genres: 529, bank: 0, program: 6, drumset: false, bass_clef: false },
    InstrumentDef { id: "virginal", name: "Virginal", trait_name: "", group: 8, genres: 512, bank: 0, program: 6, drumset: false, bass_clef: false },
    InstrumentDef { id: "electric-piano", name: "Electric Piano", trait_name: "", group: 8, genres: 6, bank: 0, program: 4, drumset: false, bass_clef: false },
    InstrumentDef { id: "grand-piano", name: "Grand Piano", trait_name: "", group: 8, genres: 48, bank: 0, program: 0, drumset: false, bass_clef: false },
    InstrumentDef { id: "honky-tonk-piano", name: "Honky Tonk Piano", trait_name: "", group: 8, genres: 6, bank: 0, program: 3, drumset: false, bass_clef: false },
    InstrumentDef { id: "piano", name: "Piano", trait_name: "", group: 8, genres: 1055, bank: 0, program: 0, drumset: false, bass_clef: false },
    InstrumentDef { id: "toy-piano", name: "Toy Piano", trait_name: "", group: 8, genres: 1024, bank: 0, program: 0, drumset: false, bass_clef: false },
    InstrumentDef { id: "upright-piano", name: "Upright Piano", trait_name: "", group: 8, genres: 0, bank: 0, program: 0, drumset: false, bass_clef: false },
    InstrumentDef { id: "hammond-organ", name: "Hammond Organ", trait_name: "", group: 8, genres: 6, bank: 0, program: 16, drumset: false, bass_clef: false },
    InstrumentDef { id: "organ", name: "Organ", trait_name: "", group: 8, genres: 23, bank: 0, program: 19, drumset: false, bass_clef: false },
    InstrumentDef { id: "percussive-organ", name: "Percussive Organ", trait_name: "", group: 8, genres: 0, bank: 0, program: 17, drumset: false, bass_clef: false },
    InstrumentDef { id: "pipe-organ", name: "Pipe Organ", trait_name: "", group: 8, genres: 17, bank: 0, program: 19, drumset: false, bass_clef: false },
    InstrumentDef { id: "rotary-organ", name: "Rotary Organ", trait_name: "", group: 8, genres: 0, bank: 0, program: 18, drumset: false, bass_clef: false },
    InstrumentDef { id: "harmonium", name: "Harmonium", trait_name: "", group: 8, genres: 0, bank: 0, program: 20, drumset: false, bass_clef: false },
    InstrumentDef { id: "reed-organ", name: "Reed Organ", trait_name: "", group: 8, genres: 0, bank: 0, program: 20, drumset: false, bass_clef: false },
    InstrumentDef { id: "mallet-synthesizer", name: "Mallet Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 80, drumset: false, bass_clef: false },
    InstrumentDef { id: "ondes-martenot", name: "Ondes Martenot", trait_name: "", group: 9, genres: 194, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "atmosphere-synth", name: "Atmosphere Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 99, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-synthesizer", name: "Bass Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 38, drumset: false, bass_clef: false },
    InstrumentDef { id: "bowed-synth", name: "Bowed Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 92, drumset: false, bass_clef: false },
    InstrumentDef { id: "brass-synthesizer", name: "Brass Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 62, drumset: false, bass_clef: false },
    InstrumentDef { id: "brightness-synth", name: "Brightness Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 100, drumset: false, bass_clef: false },
    InstrumentDef { id: "choir-synth", name: "Choir Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 91, drumset: false, bass_clef: false },
    InstrumentDef { id: "crystal-synth", name: "Crystal Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 98, drumset: false, bass_clef: false },
    InstrumentDef { id: "echoes-synth", name: "Echoes Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 102, drumset: false, bass_clef: false },
    InstrumentDef { id: "effect-synth", name: "Effect Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 96, drumset: false, bass_clef: false },
    InstrumentDef { id: "goblins-synth", name: "Goblins Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 101, drumset: false, bass_clef: false },
    InstrumentDef { id: "halo-synth", name: "Halo Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 94, drumset: false, bass_clef: false },
    InstrumentDef { id: "metallic-synth", name: "Metallic Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 93, drumset: false, bass_clef: false },
    InstrumentDef { id: "new-age-synth", name: "New Age Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 88, drumset: false, bass_clef: false },
    InstrumentDef { id: "pad-synth", name: "Pad Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 88, drumset: false, bass_clef: false },
    InstrumentDef { id: "poly-synth", name: "Poly Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 90, drumset: false, bass_clef: false },
    InstrumentDef { id: "rain-synth", name: "Rain Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 96, drumset: false, bass_clef: false },
    InstrumentDef { id: "saw-synth", name: "Saw Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 81, drumset: false, bass_clef: false },
    InstrumentDef { id: "sci-fi-synth", name: "Sci-fi Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 103, drumset: false, bass_clef: false },
    InstrumentDef { id: "sine-synth", name: "Sine Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "soundtrack-synth", name: "Soundtrack Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 97, drumset: false, bass_clef: false },
    InstrumentDef { id: "square-synth", name: "Square Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 80, drumset: false, bass_clef: false },
    InstrumentDef { id: "string-synthesizer", name: "String Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 50, drumset: false, bass_clef: false },
    InstrumentDef { id: "sweep-synth", name: "Sweep Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 95, drumset: false, bass_clef: false },
    InstrumentDef { id: "theremin", name: "Theremin", trait_name: "", group: 9, genres: 194, bank: 0, program: 79, drumset: false, bass_clef: false },
    InstrumentDef { id: "warm-synth", name: "Warm Synthesizer", trait_name: "", group: 9, genres: 194, bank: 0, program: 89, drumset: false, bass_clef: false },
    InstrumentDef { id: "harp", name: "Harp", trait_name: "", group: 10, genres: 17, bank: 0, program: 46, drumset: false, bass_clef: false },
    InstrumentDef { id: "cavaquinho", name: "Cavaquinho", trait_name: "", group: 10, genres: 256, bank: 0, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "soprano-guitar", name: "Soprano Guitar", trait_name: "", group: 10, genres: 0, bank: 0, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-guitar", name: "Alto Guitar", trait_name: "", group: 10, genres: 0, bank: 0, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "electric-guitar", name: "Electric Guitar", trait_name: "", group: 10, genres: 71, bank: 0, program: 27, drumset: false, bass_clef: false },
    InstrumentDef { id: "electric-guitar-treble-clef", name: "Electric Guitar (treble clef)", trait_name: "", group: 10, genres: 0, bank: 0, program: 27, drumset: false, bass_clef: false },
    InstrumentDef { id: "guitar-steel", name: "Acoustic Guitar", trait_name: "", group: 10, genres: 1031, bank: 0, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "guitar-steel-treble-clef", name: "Acoustic Guitar (treble clef)", trait_name: "", group: 10, genres: 0, bank: 0, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "guitar-nylon", name: "Classical Guitar", trait_name: "", group: 10, genres: 3, bank: 0, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "guitar-nylon-treble-clef", name: "Guitar (treble clef)", trait_name: "", group: 10, genres: 0, bank: 0, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "pedal-steel-guitar", name: "Pedal Steel Guitar", trait_name: "", group: 10, genres: 2, bank: 0, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "baritone-guitar", name: "Baritone Guitar", trait_name: "", group: 10, genres: 0, bank: 0, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "contra-guitar", name: "Contra Guitar", trait_name: "", group: 10, genres: 256, bank: 0, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "7-string-guitar", name: "7-string Guitar", trait_name: "", group: 10, genres: 256, bank: 0, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "11-string-alto-guitar", name: "11-string Alto Guitar", trait_name: "", group: 10, genres: 0, bank: 0, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "12-string-guitar", name: "12-string Guitar", trait_name: "", group: 10, genres: 2, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "5-string-electric-bass", name: "5-str. Electric Bass", trait_name: "", group: 10, genres: 7, bank: 0, program: 33, drumset: false, bass_clef: false },
    InstrumentDef { id: "5-string-electric-bass-high-c", name: "5-str. Electric Bass (high C/tenor)", trait_name: "", group: 10, genres: 4, bank: 0, program: 33, drumset: false, bass_clef: false },
    InstrumentDef { id: "5-string-electric-bass-tab", name: "5-str. Electric Bass", trait_name: "", group: 10, genres: 3, bank: 0, program: 33, drumset: false, bass_clef: false },
    InstrumentDef { id: "5-string-electric-bass-tab-high-c", name: "5-str. Electric Bass (high C/tenor)", trait_name: "", group: 10, genres: 4, bank: 0, program: 33, drumset: false, bass_clef: false },
    InstrumentDef { id: "6-string-electric-bass", name: "6-str. Electric Bass", trait_name: "", group: 10, genres: 4, bank: 0, program: 33, drumset: false, bass_clef: false },
    InstrumentDef { id: "6-string-electric-bass-tab", name: "6-str. Electric Bass", trait_name: "", group: 10, genres: 4, bank: 0, program: 33, drumset: false, bass_clef: false },
    InstrumentDef { id: "bass-guitar", name: "Bass Guitar", trait_name: "", group: 10, genres: 71, bank: 0, program: 34, drumset: false, bass_clef: false },
    InstrumentDef { id: "electric-bass", name: "Electric Bass", trait_name: "", group: 10, genres: 7, bank: 0, program: 33, drumset: false, bass_clef: false },
    InstrumentDef { id: "fretless-electric-bass", name: "Fretless Electric Bass", trait_name: "", group: 10, genres: 6, bank: 0, program: 35, drumset: false, bass_clef: false },
    InstrumentDef { id: "acoustic-bass", name: "Acoustic Bass", trait_name: "", group: 10, genres: 5, bank: 0, program: 32, drumset: false, bass_clef: false },
    InstrumentDef { id: "electric-bass-4-str-tab", name: "Electric Bass", trait_name: "", group: 10, genres: 3, bank: 0, program: 33, drumset: false, bass_clef: false },
    InstrumentDef { id: "banjo", name: "Banjo", trait_name: "", group: 10, genres: 7, bank: 0, program: 105, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-banjo", name: "Tenor Banjo", trait_name: "", group: 10, genres: 0, bank: 0, program: 105, drumset: false, bass_clef: false },
    InstrumentDef { id: "irish-tenor-banjo", name: "Irish Tenor Banjo", trait_name: "", group: 10, genres: 256, bank: 0, program: 105, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-ukulele", name: "Tenor Ukulele", trait_name: "", group: 10, genres: 0, bank: 8, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "ukulele", name: "Ukulele", trait_name: "", group: 10, genres: 1027, bank: 8, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "ukulele-low-g", name: "Ukulele (low G)", trait_name: "", group: 10, genres: 0, bank: 8, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "ukulele-4-str-tab", name: "Ukulele", trait_name: "", group: 10, genres: 1027, bank: 8, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "baritone-ukulele", name: "Baritone Ukulele", trait_name: "", group: 10, genres: 0, bank: 8, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "mandolin", name: "Mandolin", trait_name: "", group: 10, genres: 2, bank: 16, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-mandola", name: "Alto Mandola", trait_name: "", group: 10, genres: 0, bank: 16, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "mandola", name: "Mandola", trait_name: "", group: 10, genres: 0, bank: 16, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-mandola", name: "Tenor Mandola", trait_name: "", group: 10, genres: 0, bank: 16, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "octave-mandolin", name: "Octave Mandolin", trait_name: "", group: 10, genres: 0, bank: 16, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "mandocello", name: "Mandocello", trait_name: "", group: 10, genres: 0, bank: 16, program: 25, drumset: false, bass_clef: true },
    InstrumentDef { id: "mtn-dulcimer-std", name: "Mtn. Dulcimer", trait_name: "", group: 10, genres: 256, bank: 0, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "mtn-dulcimer-std-chrom-tab", name: "Mtn. Dulcimer", trait_name: "", group: 10, genres: 256, bank: 0, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "mtn-dulcimer-baritone", name: "Mtn. Dulcimer - Baritone", trait_name: "", group: 10, genres: 256, bank: 0, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "mtn-dulcimer-bartn-chrom-tab", name: "Mtn. Dulcimer - Baritone", trait_name: "", group: 10, genres: 256, bank: 0, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "mtn-dulcimer-bass", name: "Mtn. Dulcimer - Bass", trait_name: "", group: 10, genres: 256, bank: 0, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "mtn-dulcimer-bass-chrom-tab", name: "Mtn. Dulcimer - Bass", trait_name: "", group: 10, genres: 256, bank: 0, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "lute", name: "Lute", trait_name: "", group: 10, genres: 512, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "ren.-tenor-lute-5-course", name: "Tenor Lute", trait_name: "5-course", group: 10, genres: 512, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "ren.-tenor-lute-6-course", name: "Tenor Lute", trait_name: "6-course", group: 10, genres: 512, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "ren.-tenor-lute-7-course", name: "Tenor Lute", trait_name: "7-course", group: 10, genres: 512, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "ren.-tenor-lute-8-course", name: "Tenor Lute", trait_name: "8-course", group: 10, genres: 512, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "ren.-tenor-lute-9-course", name: "Tenor Lute", trait_name: "9-course", group: 10, genres: 512, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "ren.-tenor-lute-10-course", name: "Tenor Lute", trait_name: "10-course", group: 10, genres: 512, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "baroque-lute-13-course", name: "Tenor Lute", trait_name: "13-course", group: 10, genres: 512, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "theorbo-14-course", name: "Theorbo", trait_name: "", group: 10, genres: 512, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "archlute-14-course", name: "Archlute", trait_name: "", group: 10, genres: 512, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "balalaika-piccolo", name: "Piccolo Balalaika", trait_name: "", group: 10, genres: 0, bank: 0, program: 107, drumset: false, bass_clef: false },
    InstrumentDef { id: "balalaika", name: "balalaika", trait_name: "", group: 10, genres: 256, bank: 0, program: 107, drumset: false, bass_clef: false },
    InstrumentDef { id: "balalaika-prima", name: "Prima Balalaika", trait_name: "", group: 10, genres: 0, bank: 0, program: 107, drumset: false, bass_clef: false },
    InstrumentDef { id: "balalaika-secunda", name: "Secunda Balalaika", trait_name: "", group: 10, genres: 0, bank: 0, program: 107, drumset: false, bass_clef: false },
    InstrumentDef { id: "balalaika-alto", name: "Alto Balalaika", trait_name: "", group: 10, genres: 0, bank: 0, program: 107, drumset: false, bass_clef: false },
    InstrumentDef { id: "balalaika-bass", name: "Bass Balalaika", trait_name: "", group: 10, genres: 0, bank: 0, program: 107, drumset: false, bass_clef: true },
    InstrumentDef { id: "balalaika-contrabass", name: "Contrabass Balalaika", trait_name: "", group: 10, genres: 0, bank: 0, program: 107, drumset: false, bass_clef: true },
    InstrumentDef { id: "bouzouki-3-course", name: "Bouzouki", trait_name: "3-course", group: 10, genres: 256, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "bouzouki-4-course", name: "Bouzouki", trait_name: "4-course", group: 10, genres: 256, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "guzheng", name: "Guzheng", trait_name: "", group: 10, genres: 256, bank: 0, program: 107, drumset: false, bass_clef: false },
    InstrumentDef { id: "guqin", name: "Guqin", trait_name: "", group: 10, genres: 256, bank: 0, program: 107, drumset: false, bass_clef: true },
    InstrumentDef { id: "pipa", name: "Pipa", trait_name: "", group: 10, genres: 256, bank: 0, program: 107, drumset: false, bass_clef: false },
    InstrumentDef { id: "yangqin", name: "Yangqin", trait_name: "", group: 10, genres: 256, bank: 0, program: 107, drumset: false, bass_clef: false },
    InstrumentDef { id: "gayageum", name: "Gayageum", trait_name: "", group: 10, genres: 256, bank: 0, program: 107, drumset: false, bass_clef: false },
    InstrumentDef { id: "koto", name: "Koto", trait_name: "", group: 10, genres: 256, bank: 0, program: 107, drumset: false, bass_clef: false },
    InstrumentDef { id: "oud", name: "Oud", trait_name: "", group: 10, genres: 768, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "shamisen", name: "Shamisen", trait_name: "", group: 10, genres: 256, bank: 0, program: 106, drumset: false, bass_clef: false },
    InstrumentDef { id: "sitar", name: "Sitar", trait_name: "", group: 10, genres: 256, bank: 0, program: 104, drumset: false, bass_clef: false },
    InstrumentDef { id: "prim", name: "Prim", trait_name: "", group: 10, genres: 256, bank: 0, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "brac", name: "Brac", trait_name: "", group: 10, genres: 256, bank: 0, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "bugarija", name: "Bugarija", trait_name: "", group: 10, genres: 256, bank: 0, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "berda", name: "Berda", trait_name: "", group: 10, genres: 256, bank: 0, program: 32, drumset: false, bass_clef: true },
    InstrumentDef { id: "celo", name: "Celo", trait_name: "", group: 10, genres: 256, bank: 0, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "bandurria", name: "Bandurria", trait_name: "", group: 10, genres: 2, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "laud", name: "Laud", trait_name: "", group: 10, genres: 2, bank: 8, program: 25, drumset: false, bass_clef: false },
    InstrumentDef { id: "timple-canario", name: "Canarian Timple", trait_name: "", group: 10, genres: 256, bank: 8, program: 24, drumset: false, bass_clef: false },
    InstrumentDef { id: "strings", name: "Strings", trait_name: "", group: 11, genres: 0, bank: 0, program: 48, drumset: false, bass_clef: false },
    InstrumentDef { id: "double-bass", name: "double-bass", trait_name: "", group: 11, genres: 0, bank: 0, program: 43, drumset: false, bass_clef: false },
    InstrumentDef { id: "violin", name: "Violin", trait_name: "", group: 11, genres: 23, bank: 0, program: 40, drumset: false, bass_clef: false },
    InstrumentDef { id: "violins", name: "Violins (section)", trait_name: "", group: 11, genres: 17, bank: 21, program: 48, drumset: false, bass_clef: false },
    InstrumentDef { id: "viola", name: "Viola", trait_name: "", group: 11, genres: 21, bank: 0, program: 41, drumset: false, bass_clef: false },
    InstrumentDef { id: "violas", name: "Violas (section)", trait_name: "", group: 11, genres: 17, bank: 31, program: 48, drumset: false, bass_clef: false },
    InstrumentDef { id: "violoncello", name: "Violoncello", trait_name: "", group: 11, genres: 23, bank: 0, program: 42, drumset: false, bass_clef: true },
    InstrumentDef { id: "violoncellos", name: "Violoncellos (section)", trait_name: "", group: 11, genres: 17, bank: 41, program: 48, drumset: false, bass_clef: true },
    InstrumentDef { id: "contrabass", name: "Contrabass", trait_name: "", group: 11, genres: 53, bank: 0, program: 43, drumset: false, bass_clef: false },
    InstrumentDef { id: "contrabasses", name: "Contrabasses (section)", trait_name: "", group: 11, genres: 17, bank: 51, program: 48, drumset: false, bass_clef: false },
    InstrumentDef { id: "treble-viol", name: "Treble Viol", trait_name: "", group: 11, genres: 512, bank: 0, program: 40, drumset: false, bass_clef: false },
    InstrumentDef { id: "alto-viol", name: "Alto Viol", trait_name: "", group: 11, genres: 512, bank: 0, program: 41, drumset: false, bass_clef: false },
    InstrumentDef { id: "pardessus-de-viole", name: "Pardessus de viole", trait_name: "", group: 11, genres: 512, bank: 0, program: 40, drumset: false, bass_clef: false },
    InstrumentDef { id: "tenor-viol", name: "Tenor Viol", trait_name: "", group: 11, genres: 512, bank: 0, program: 41, drumset: false, bass_clef: false },
    InstrumentDef { id: "baryton", name: "Baryton", trait_name: "", group: 11, genres: 512, bank: 0, program: 42, drumset: false, bass_clef: true },
    InstrumentDef { id: "viola-da-gamba", name: "Bass Viol", trait_name: "", group: 11, genres: 512, bank: 0, program: 42, drumset: false, bass_clef: true },
    InstrumentDef { id: "violone", name: "Violone", trait_name: "G", group: 11, genres: 512, bank: 0, program: 43, drumset: false, bass_clef: true },
    InstrumentDef { id: "d-violone", name: "Violone", trait_name: "D", group: 11, genres: 512, bank: 0, program: 43, drumset: false, bass_clef: true },
    InstrumentDef { id: "octobass", name: "Octobass", trait_name: "", group: 11, genres: 0, bank: 0, program: 43, drumset: false, bass_clef: false },
    InstrumentDef { id: "erhu", name: "Erhu", trait_name: "", group: 11, genres: 256, bank: 0, program: 110, drumset: false, bass_clef: false },
    InstrumentDef { id: "nyckelharpa", name: "Nyckelharpa", trait_name: "", group: 11, genres: 256, bank: 0, program: 41, drumset: false, bass_clef: false },
    InstrumentDef { id: "ajaeng", name: "Ajaeng", trait_name: "", group: 11, genres: 256, bank: 0, program: 110, drumset: false, bass_clef: true },
];

/// Index of the genre MuseScore lists first and the picker opens on.
pub const COMMON_GENRE: usize = 0;

pub fn instrument(id: &str) -> Option<&'static InstrumentDef> {
    let found = |id: &str| INSTRUMENTS.iter().find(|def| def.id == id);
    // Notation-only twins are not listed any more; a name from an older file
    // finds the instrument it was a twin of.
    found(id).or_else(|| {
        ["-tablature", "-single"]
            .iter()
            .find_map(|suffix| id.strip_suffix(suffix).and_then(found))
    })
}

impl InstrumentDef {
    /// The name with its trait, as the picker lists it ("Piccolo (Eb)").
    pub fn title(&self) -> String {
        match self.trait_name {
            "" => self.name.to_owned(),
            text if text.starts_with('(') => format!("{} {text}", self.name),
            text => format!("{} ({text})", self.name),
        }
    }

    /// A guitar or bass of the plucked family, which is what the tab view
    /// and the fretboard are built for.
    pub fn is_stringed(&self) -> bool {
        GROUPS[usize::from(self.group)].id == "plucked-strings"
    }
}

/// Instruments the picker shows for a genre (`None` is "All instruments")
/// and a group (`None` is every group), in MuseScore's order. A search text
/// lists the instruments whose name holds it, the earliest match first.
pub fn listed(
    genre: Option<usize>,
    group: Option<usize>,
    search: &str,
) -> Vec<&'static InstrumentDef> {
    let needle = search.trim().to_lowercase();
    let mut found: Vec<(usize, &'static InstrumentDef)> = INSTRUMENTS
        .iter()
        .filter(|def| genre.is_none_or(|genre| def.genres & (1 << genre) != 0))
        .filter(|def| group.is_none_or(|group| usize::from(def.group) == group))
        .filter_map(|def| {
            if needle.is_empty() {
                return Some((0, def));
            }
            def.title().to_lowercase().find(&needle).map(|at| (at, def))
        })
        .collect();
    // Stable, so equal matches keep the source order.
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, def)| def).collect()
}

/// Groups that hold at least one instrument of the genre, in source order.
pub fn groups_of(genre: Option<usize>) -> Vec<usize> {
    (0..GROUPS.len())
        .filter(|group| !listed(genre, Some(*group), "").is_empty())
        .collect()
}

/// Playing techniques that sound a note with another preset than the
/// instrument's usual one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Technique {
    /// Palm muting.
    Mute,
    Slap,
    Pop,
}

/// The preset (bank, program) each technique uses on the instruments that
/// define one, from the instrument's technique channels in instruments.xml.
static TECHNIQUE_SOUNDS: &[(&str, Technique, u16, u8)] = &[
    ("soprano-guitar", Technique::Mute, 0, 28),
    ("alto-guitar", Technique::Mute, 0, 28),
    ("electric-guitar", Technique::Mute, 0, 28),
    ("guitar-steel", Technique::Mute, 0, 28),
    ("guitar-nylon", Technique::Mute, 0, 28),
    ("guitar-nylon-treble-clef", Technique::Mute, 0, 28),
    ("pedal-steel-guitar", Technique::Mute, 0, 28),
    ("contra-guitar", Technique::Mute, 0, 28),
    ("11-string-alto-guitar", Technique::Mute, 0, 28),
    ("12-string-guitar", Technique::Mute, 0, 28),
    ("5-string-electric-bass", Technique::Slap, 0, 36),
    ("5-string-electric-bass", Technique::Pop, 0, 37),
    ("5-string-electric-bass-high-c", Technique::Slap, 0, 36),
    ("5-string-electric-bass-high-c", Technique::Pop, 0, 37),
    ("5-string-electric-bass-tab", Technique::Slap, 0, 36),
    ("5-string-electric-bass-tab", Technique::Pop, 0, 37),
    ("5-string-electric-bass-tab-high-c", Technique::Slap, 0, 36),
    ("5-string-electric-bass-tab-high-c", Technique::Pop, 0, 37),
    ("6-string-electric-bass", Technique::Slap, 0, 36),
    ("6-string-electric-bass", Technique::Pop, 0, 37),
    ("6-string-electric-bass-tab", Technique::Slap, 0, 36),
    ("6-string-electric-bass-tab", Technique::Pop, 0, 37),
    ("bass-guitar", Technique::Slap, 0, 36),
    ("bass-guitar", Technique::Pop, 0, 37),
    ("electric-bass", Technique::Slap, 0, 36),
    ("electric-bass", Technique::Pop, 0, 37),
    ("fretless-electric-bass", Technique::Slap, 0, 36),
    ("fretless-electric-bass", Technique::Pop, 0, 37),
    ("acoustic-bass", Technique::Slap, 0, 36),
    ("acoustic-bass", Technique::Pop, 0, 37),
    ("electric-bass-4-str-tab", Technique::Slap, 0, 36),
    ("electric-bass-4-str-tab", Technique::Pop, 0, 37),
];

/// The preset `technique` sounds with on an instrument, when it has one.
pub fn technique_sound(id: &str, technique: Technique) -> Option<(u16, u8)> {
    TECHNIQUE_SOUNDS
        .iter()
        .find(|(candidate, kind, ..)| *candidate == id && *kind == technique)
        .map(|&(.., bank, program)| (bank, program))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_instrument_has_a_unique_id_and_a_real_group() {
        let mut ids: Vec<&str> = INSTRUMENTS.iter().map(|def| def.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), INSTRUMENTS.len());
        assert!(INSTRUMENTS.iter().all(|def| usize::from(def.group) < GROUPS.len()));
        assert!(INSTRUMENTS.iter().all(|def| def.program < 128));
    }

    #[test]
    fn common_lists_the_everyday_instruments_in_source_order() {
        let common = listed(Some(COMMON_GENRE), None, "");
        assert!(common.iter().any(|def| def.id == "electric-guitar"));
        assert!(common.len() < INSTRUMENTS.len());
        let all = listed(None, None, "");
        assert_eq!(all.len(), INSTRUMENTS.len());
        let position = |id: &str| all.iter().position(|def| def.id == id).unwrap();
        assert!(position("winds") < position("electric-guitar"));
    }

    #[test]
    fn a_search_puts_the_earliest_match_first() {
        let found = listed(None, None, "guitar");
        assert!(!found.is_empty());
        let first = found[0].title().to_lowercase();
        assert!(first.starts_with("guitar") || first.find("guitar").unwrap() <= 4);
    }

    #[test]
    fn notation_only_twins_are_not_listed() {
        assert!(INSTRUMENTS.iter().all(|def| !def.id.ends_with("-tablature")));
        assert!(INSTRUMENTS.iter().all(|def| !def.name.contains("tablature")));
        assert!(INSTRUMENTS.iter().all(|def| !def.name.contains("staff")));
        // An id from an older file finds the instrument it was a twin of.
        let base = instrument("guitar-nylon").unwrap();
        assert_eq!(instrument("guitar-nylon-tablature").map(|def| def.id), Some(base.id));
        assert_eq!(instrument("marimba-single").map(|def| def.id), Some("marimba"));
        assert_eq!(instrument("marimba").unwrap().name, "Marimba");
    }

    #[test]
    fn drum_kits_choose_the_drum_bank() {
        let kit = instrument("drumset").unwrap();
        assert!(kit.drumset);
        assert_eq!(kit.bank, 128);
    }

    #[test]
    fn groups_follow_the_genre() {
        let all = groups_of(None);
        assert_eq!(all.len(), GROUPS.len());
        assert!(groups_of(Some(COMMON_GENRE)).len() <= all.len());
    }

    #[test]
    fn technique_sounds_belong_to_listed_instruments_and_existing_presets() {
        use crate::core::sounds::preset_name;
        for &(id, _, bank, program) in TECHNIQUE_SOUNDS {
            assert!(instrument(id).is_some(), "{id}");
            assert!(preset_name(bank, program).is_some(), "{id} {bank}/{program}");
        }
    }

    #[test]
    fn guitars_mute_and_basses_slap_and_pop() {
        assert_eq!(technique_sound("electric-guitar", Technique::Mute), Some((0, 28)));
        assert_eq!(technique_sound("guitar-nylon", Technique::Mute), Some((0, 28)));
        assert_eq!(technique_sound("electric-bass", Technique::Slap), Some((0, 36)));
        assert_eq!(technique_sound("electric-bass-4-str-tab", Technique::Pop), Some((0, 37)));
        assert_eq!(technique_sound("electric-bass", Technique::Mute), None);
        assert_eq!(technique_sound("electric-guitar", Technique::Slap), None);
        assert_eq!(technique_sound("flute", Technique::Mute), None);
    }

    #[test]
    fn a_plucked_acoustic_bass_starts_as_an_acoustic_bass() {
        let def = instrument("acoustic-bass").unwrap();
        assert_eq!((def.bank, def.program), (0, 32));
    }
}
