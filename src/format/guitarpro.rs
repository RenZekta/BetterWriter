//! Guitar Pro 7/8 export: a zip holding the score as GPIF XML.
//!
//! Strings are numbered from the lowest one, the reverse of the track's
//! tuning order, and a tuning lists its pitches lowest first.

use std::io::{Cursor, Write};

use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::core::{BwxProject, pitch::PitchEnvelope, sounds::midi_pan};

use super::{
    FormatError,
    xml::Xml,
    score::{self, ExportBeat, ExportNote, ExportSong, GridBar, TempoEvent},
};

/// Writes `project` as a Guitar Pro 7/8 file.
pub fn write_gp(project: &BwxProject) -> Result<Vec<u8>, FormatError> {
    let song = score::prepare(project)?;
    let xml = gpif(&song);

    let mut archive = ZipWriter::new(Cursor::new(Vec::<u8>::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive.start_file("VERSION", options)?;
    archive.write_all(b"7.0")?;
    archive.start_file("Content/score.gpif", options)?;
    archive.write_all(xml.as_bytes())?;
    Ok(archive.finish()?.into_inner())
}

fn gpif(song: &ExportSong) -> String {
    let mut xml = Xml::new();
    xml.open("GPIF");
    xml.text("GPVersion", "8.1.3");
    xml.text_with(
        "GPRevision",
        &[
            ("required", "12024".to_owned()),
            ("recommended", "13000".to_owned()),
        ],
        "13007",
    );
    xml.open("Encoding");
    xml.text("EncodingDescription", "GP8");
    xml.close("Encoding");

    write_score(&mut xml, song.project);
    write_master_track(&mut xml, song);
    xml.empty("AudioTracks");
    write_tracks(&mut xml, song);

    let ids = Ids::assign(song);
    write_master_bars(&mut xml, song, &ids);
    xml.out.push_str(&ids.bars);
    xml.out.push_str(&ids.voices);
    xml.out.push_str(&ids.beats);
    xml.out.push_str(&ids.notes);
    xml.out.push_str(&ids.rhythms);
    xml.close("GPIF");
    xml.out
}

fn write_score(xml: &mut Xml, project: &BwxProject) {
    xml.open("Score");
    xml.cdata("Title", &project.title);
    for tag in [
        "SubTitle",
        "Artist",
        "Album",
        "Words",
        "Music",
        "WordsAndMusic",
        "Copyright",
        "Tabber",
        "Instructions",
        "Notices",
        "FirstPageHeader",
        "FirstPageFooter",
        "PageHeader",
        "PageFooter",
    ] {
        xml.cdata(tag, "");
    }
    xml.text("ScoreSystemsDefaultLayout", "4");
    xml.text("ScoreSystemsLayout", "");
    xml.text("ScoreZoomPolicy", "Value");
    xml.text("ScoreZoom", "1");
    xml.text("MultiVoice", "1>");
    xml.close("Score");
}

/// The bar holding `tick` and how far into it the tick lies.
fn locate(grid: &[GridBar], tick: crate::core::Tick) -> Option<(usize, f64)> {
    let index = grid.iter().rposition(|bar| bar.start <= tick)?;
    let bar = &grid[index];
    (tick < bar.end).then(|| (index, score::bar_fraction(bar, tick)))
}

fn write_tempo(xml: &mut Xml, bar: usize, position: f64, event: &TempoEvent) {
    xml.open("Automation");
    xml.text("Type", "Tempo");
    xml.text("Linear", event.linear);
    xml.text("Bar", bar);
    xml.text("Position", position);
    xml.text("Visible", "true");
    xml.text("Value", format!("{} 2", event.bpm.round().clamp(1.0, 999.0) as u16));
    xml.close("Automation");
}

fn write_master_track(xml: &mut Xml, song: &ExportSong) {
    xml.open("MasterTrack");
    let tracks: Vec<String> = (0..song.tracks.len()).map(|index| index.to_string()).collect();
    xml.text("Tracks", tracks.join(" "));
    xml.open("Automations");
    let mut wrote_first = false;
    for event in &song.tempos {
        if let Some((bar, position)) = locate(&song.grid, event.tick) {
            wrote_first |= bar == 0 && position == 0.0;
            write_tempo(xml, bar, position, event);
        }
    }
    if !wrote_first {
        let opening = TempoEvent {
            tick: 0,
            bpm: f32::from(song.bar_tempo(0)),
            linear: false,
        };
        write_tempo(xml, 0, 0.0, &opening);
    }
    xml.close("Automations");
    xml.close("MasterTrack");
}

/// Icon and instrument-set name Guitar Pro shows for a MIDI program.
fn instrument_look(program: u8) -> (u8, &'static str) {
    match program {
        24 => (23, "Nylon Guitar"),
        25 => (1, "Steel Guitar"),
        27..=29 => (4, "Electric Guitar"),
        26 | 30 | 31 => (1, "Electric Guitar"),
        32 | 35 => (5, "Acoustic Bass"),
        33 | 34 | 36 | 37 => (5, "Electric Bass"),
        38 | 39 => (12, "Synth Bass"),
        _ => (1, "Steel Guitar"),
    }
}

/// `"Electric Guitar"` becomes `electricGuitar`.
fn instrument_set_type(name: &str) -> String {
    let mut parts = name.split(' ');
    let mut out = parts.next().unwrap_or("").to_lowercase();
    for part in parts {
        out.push_str(part);
    }
    out
}

fn tuning_instrument(strings: usize, program: u8) -> &'static str {
    match strings {
        3 => "Shamisen",
        4 | 5 if program == 105 => "Banjo",
        4 | 5 => "Bass",
        6 | 7 if program <= 39 => "Bass",
        _ => "Guitar",
    }
}

fn short_name(name: &str) -> String {
    name.chars().filter(|ch| !ch.is_whitespace()).take(4).collect()
}

fn write_tracks(xml: &mut Xml, song: &ExportSong) {
    xml.open("Tracks");
    for (index, exported) in song.tracks.iter().enumerate() {
        let track = exported.track;
        let (bank, program) = track.sound();
        let program = program.min(127);
        let (icon, set_name) = instrument_look(program);
        let channel = track.channel.min(15);

        xml.open_with("Track", &[("id", index.to_string())]);
        xml.cdata("Name", &track.name);
        xml.cdata("ShortName", &short_name(&track.name));
        xml.text("Color", "0 0 0");
        xml.text("SystemsDefautLayout", "3");
        xml.text("SystemsLayout", "");
        xml.empty("AutoBrush");
        xml.text("PalmMute", "0");
        xml.text("PlayingStyle", "StringedPick");
        xml.empty("UseOneChannelPerString");
        xml.text("IconId", icon);

        xml.open("InstrumentSet");
        xml.text("Name", set_name);
        xml.text("Type", instrument_set_type(set_name));
        xml.text("LineCount", 5);
        xml.open("Elements");
        xml.open("Element");
        xml.text("Name", "Pitched");
        xml.text("Type", "pitched");
        xml.text("SoundbankName", "");
        xml.open("Articulations");
        xml.open("Articulation");
        xml.text("Name", "");
        xml.text("StaffLine", 0);
        xml.text("Noteheads", "noteheadBlack noteheadHalf noteheadWhole");
        xml.text("TechniquePlacement", "outside");
        xml.text("TechniqueSymbol", "");
        xml.text("InputMidiNumbers", "");
        xml.text("OutputRSESound", "");
        xml.text("OutputMidiNumber", 0);
        xml.close("Articulation");
        xml.close("Articulations");
        xml.close("Element");
        xml.close("Elements");
        xml.close("InstrumentSet");

        xml.open("Transpose");
        xml.text("Chromatic", 0);
        xml.text("Octave", 0);
        xml.close("Transpose");

        xml.open("RSE");
        xml.open_with("ChannelStrip", &[("version", "E56".to_owned())]);
        xml.text(
            "Parameters",
            format!(
                "0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5 1 0.5 {} {} 0.5 0.5 0.5",
                f32::from(midi_pan(track.pan)) / 127.0,
                f32::from(track.volume.min(127)) / 127.0
            ),
        );
        xml.close("ChannelStrip");
        xml.close("RSE");
        xml.text("ForcedSound", -1);

        xml.open("MidiConnection");
        xml.text("Port", 0);
        xml.text("PrimaryChannel", channel);
        xml.text("SecondaryChannel", (channel + 1) % 16);
        xml.text("ForeOneChannelPerString", "false");
        xml.close("MidiConnection");

        xml.text(
            "PlaybackState",
            if track.solo {
                "Solo"
            } else if track.mute {
                "Mute"
            } else {
                "Default"
            },
        );
        xml.text("AudioEngineState", "MIDI");
        xml.open("Staves");
        xml.open("Staff");
        xml.open("Properties");
        xml.property("CapoFret", "Fret", Some("0".to_owned()));
        xml.property("FretCount", "Fret", Some(track.fret_count.max(1).to_string()));
        let pitches: Vec<String> = track.tuning.iter().rev().map(u8::to_string).collect();
        xml.open_with("Property", &[("name", "Tuning".to_owned())]);
        xml.text("Pitches", pitches.join(" "));
        xml.cdata("Label", "");
        xml.text("LabelVisible", "false");
        xml.empty("Flat");
        xml.text("Instrument", tuning_instrument(track.tuning.len(), program));
        xml.close("Property");
        xml.property("PartialCapoFret", "Fret", Some("0".to_owned()));
        xml.property(
            "PartialCapoStringFlags",
            "Bitset",
            Some("0".repeat(track.tuning.len())),
        );
        xml.property("TuningFlat", "Enable", None);
        for name in ["DiagramCollection", "DiagramWorkingSet"] {
            xml.open_with("Property", &[("name", name.to_owned())]);
            xml.empty("Items");
            xml.close("Property");
        }
        xml.close("Properties");
        xml.close("Staff");
        xml.close("Staves");

        let sound_name = format!("Track_{index}_Initial");
        let path = format!("Midi/{program}");
        xml.open("Sounds");
        xml.open("Sound");
        xml.cdata("Name", &sound_name);
        xml.cdata("Label", &sound_name);
        xml.cdata("Path", &path);
        xml.cdata("Role", "Factory");
        xml.open("MIDI");
        xml.text("LSB", bank & 0x7f);
        xml.text("MSB", (bank >> 7) & 0x7f);
        xml.text("Program", program);
        xml.close("MIDI");
        xml.close("Sound");
        xml.close("Sounds");
        xml.open("Automations");
        xml.open("Automation");
        xml.text("Type", "Sound");
        xml.text("Linear", "false");
        xml.text("Bar", 0);
        xml.text("Position", 0);
        xml.text("Visible", "true");
        xml.cdata("Value", &format!("{path};{sound_name};Factory"));
        xml.close("Automation");
        xml.close("Automations");
        xml.close("Track");
    }
    xml.close("Tracks");
}

fn write_master_bars(xml: &mut Xml, song: &ExportSong, ids: &Ids) {
    xml.open("MasterBars");
    for (index, bar) in song.grid.iter().enumerate() {
        xml.open("MasterBar");
        xml.open("Key");
        xml.text("AccidentalCount", 0);
        xml.text("Mode", "Major");
        xml.text("Sharps", "Sharps");
        xml.close("Key");
        xml.text(
            "Time",
            format!("{}/{}", bar.signature.numerator, bar.signature.denominator),
        );
        let bar_ids: Vec<String> = ids.bar_ids.iter().map(|track| track[index].to_string()).collect();
        xml.text("Bars", bar_ids.join(" "));
        if bar.repeat_open || bar.repeat_close > 0 {
            let mut attributes = vec![
                ("start", bar.repeat_open.to_string()),
                ("end", (bar.repeat_close > 0).to_string()),
            ];
            if bar.repeat_close > 0 {
                attributes.push(("count", (u32::from(bar.repeat_close) + 1).to_string()));
            }
            xml.empty_with("Repeat", &attributes);
        }
        xml.close("MasterBar");
    }
    xml.close("MasterBars");
}

fn dynamic(velocity: u8) -> &'static str {
    const NAMES: [&str; 8] = ["PPP", "PP", "P", "MP", "MF", "F", "FF", "FFF"];
    let index = ((i32::from(velocity) - 15 + 8) / 16).clamp(0, 7);
    NAMES[index as usize]
}

fn note_value(value: u32) -> &'static str {
    match value {
        1 => "Whole",
        2 => "Half",
        4 => "Quarter",
        8 => "Eighth",
        16 => "16th",
        32 => "32nd",
        _ => "64th",
    }
}

/// The XML of the bars, voices, beats, notes and rhythms sections, built
/// together because their ids reference each other.
struct Ids {
    bar_ids: Vec<Vec<usize>>,
    bars: String,
    voices: String,
    beats: String,
    notes: String,
    rhythms: String,
}

impl Ids {
    fn assign(song: &ExportSong) -> Self {
        let mut bars = Xml::new();
        let mut voices = Xml::new();
        let mut beats = Xml::new();
        let mut notes = Xml::new();
        let mut rhythm_xml = Xml::new();
        for section in [&mut bars, &mut voices, &mut beats, &mut notes, &mut rhythm_xml] {
            section.out.clear();
            section.depth = 1;
        }
        bars.open("Bars");
        voices.open("Voices");
        beats.open("Beats");
        notes.open("Notes");
        rhythm_xml.open("Rhythms");

        // A rhythm is a note value, its dots and the tuplet it is in.
        let mut rhythms: Vec<(u32, u8, Option<(u8, u8)>)> = Vec::new();
        let mut bar_ids = Vec::new();
        let (mut next_bar, mut next_voice, mut next_beat, mut next_note) = (0, 0, 0, 0);

        for exported in &song.tracks {
            let flat: Vec<&ExportBeat> = exported
                .bars
                .iter()
                .flat_map(|bar| bar.beats.iter())
                .collect();
            let mut cursor: usize = 0;
            let mut ids_of_track = Vec::new();
            let clef = if exported.track.program.min(127) >= 32 && exported.track.program <= 39 {
                "F4"
            } else {
                "G2"
            };
            for bar in &exported.bars {
                let bar_id = next_bar;
                let voice_id = next_voice;
                next_bar += 1;
                next_voice += 1;
                ids_of_track.push(bar_id);

                let mut beat_ids = Vec::new();
                for beat in &bar.beats {
                    let beat_id = next_beat;
                    next_beat += 1;
                    beat_ids.push(beat_id);

                    let (value, dots) = score::value_and_dots(beat.written);
                    let tuplet = beat.tuplet.map(|tuplet| (tuplet.enters, tuplet.times));
                    let key = (value, dots, tuplet);
                    let rhythm = rhythms.iter().position(|&known| known == key).unwrap_or_else(|| {
                        rhythms.push(key);
                        rhythm_xml.open_with("Rhythm", &[("id", (rhythms.len() - 1).to_string())]);
                        if dots > 0 {
                            rhythm_xml.empty_with("AugmentationDot", &[("count", dots.to_string())]);
                        }
                        rhythm_xml.text("NoteValue", note_value(value));
                        if let Some((enters, times)) = tuplet {
                            rhythm_xml.empty_with(
                                "PrimaryTuplet",
                                &[("num", enters.to_string()), ("den", times.to_string())],
                            );
                        }
                        rhythm_xml.close("Rhythm");
                        rhythms.len() - 1
                    });

                    let previous = cursor.checked_sub(1).map(|at| flat[at]);
                    let following = flat.get(cursor + 1).copied();
                    let mut note_ids = Vec::new();
                    for note in &beat.notes {
                        note_ids.push(next_note);
                        write_note(
                            &mut notes,
                            next_note,
                            exported.track.tuning.as_slice(),
                            note,
                            previous,
                            following,
                        );
                        next_note += 1;
                    }
                    cursor += 1;

                    beats.open_with("Beat", &[("id", beat_id.to_string())]);
                    let loudest = beat.notes.iter().map(|note| note.velocity).max();
                    beats.text("Dynamic", dynamic(loudest.unwrap_or(79)));
                    if beat.notes.iter().any(|note| note.effects.fade_in) {
                        beats.text("Fadding", "FadeIn");
                    }
                    beats.empty_with("Rhythm", &[("ref", rhythm.to_string())]);
                    if !note_ids.is_empty() {
                        let list: Vec<String> = note_ids.iter().map(usize::to_string).collect();
                        beats.text("Notes", list.join(" "));
                    }
                    let slap = beat.notes.iter().any(|note| note.effects.slapping);
                    let pop = beat.notes.iter().any(|note| note.effects.popping);
                    let whammy = beat.notes.iter().find_map(|note| note.tremolo.as_ref());
                    if slap || pop || whammy.is_some() {
                        beats.open("Properties");
                        if let Some(whammy) = whammy {
                            write_pitch_properties(&mut beats, "WhammyBar", "WhammyBar", whammy);
                        }
                        if slap {
                            beats.property("Slapped", "Enable", None);
                        }
                        if pop {
                            beats.property("Popped", "Enable", None);
                        }
                        beats.close("Properties");
                    }
                    let texts = exported.texts_in(beat);
                    if !texts.is_empty() {
                        beats.text("FreeText", texts.join(" "));
                    }
                    beats.close("Beat");
                }

                voices.open_with("Voice", &[("id", voice_id.to_string())]);
                let list: Vec<String> = beat_ids.iter().map(usize::to_string).collect();
                voices.text("Beats", list.join(" "));
                voices.close("Voice");

                bars.open_with("Bar", &[("id", bar_id.to_string())]);
                bars.text("Voices", format!("{voice_id} -1 -1 -1"));
                bars.text("Clef", clef);
                bars.close("Bar");
            }
            bar_ids.push(ids_of_track);
        }
        bars.close("Bars");
        voices.close("Voices");
        beats.close("Beats");
        notes.close("Notes");
        rhythm_xml.close("Rhythms");
        Self {
            bar_ids,
            bars: bars.out,
            voices: voices.out,
            beats: beats.out,
            notes: notes.out,
            rhythms: rhythm_xml.out,
        }
    }
}

/// Guitar Pro counts a bend in hundredths of a whole tone: 25 to a quarter tone.
pub(super) const GP_QUARTER_TONE: f32 = 25.0;

/// Writes a bend (`prefix` Bend, `enable` Bended) or the beat's whammy bar
/// (`prefix` WhammyBar, `enable` WhammyBar) the way Guitar Pro keeps either:
/// an origin, a destination and one pitch held in between.
fn write_pitch_properties(xml: &mut Xml, prefix: &str, enable: &str, envelope: &PitchEnvelope) {
    let shape = envelope.plateau(envelope.length());
    let float = |value: f32| Some(format!("{value}"));
    xml.property(enable, "Enable", None);
    xml.property(&format!("{prefix}OriginValue"), "Float", float(f32::from(shape.origin) * GP_QUARTER_TONE));
    xml.property(&format!("{prefix}OriginOffset"), "Float", float(0.0));
    if let Some((value, from, to)) = shape.middle {
        xml.property(&format!("{prefix}MiddleValue"), "Float", float(f32::from(value) * GP_QUARTER_TONE));
        xml.property(&format!("{prefix}MiddleOffset1"), "Float", float(from as f32));
        xml.property(&format!("{prefix}MiddleOffset2"), "Float", float(to as f32));
    }
    xml.property(&format!("{prefix}DestinationValue"), "Float", float(f32::from(shape.destination) * GP_QUARTER_TONE));
    xml.property(&format!("{prefix}DestinationOffset"), "Float", float(100.0));
}

fn on_string(beat: Option<&ExportBeat>, string_index: usize) -> Option<&ExportNote> {
    beat?.notes.iter().find(|note| note.string_index == string_index)
}

/// `previous` and `following` are the beats around the note's own on its
/// track, which tie and hammer-on links point at.
fn write_note(
    xml: &mut Xml,
    id: usize,
    tuning: &[u8],
    note: &ExportNote,
    previous: Option<&ExportBeat>,
    following: Option<&ExportBeat>,
) {
    let effects = &note.effects;
    let strings = tuning.len();
    let pitch = tuning
        .get(note.string_index)
        .copied()
        .unwrap_or(40)
        .saturating_add(note.fret)
        .min(127);

    let next = on_string(following, note.string_index);
    let before = on_string(previous, note.string_index);
    let tie_origin = next.is_some_and(|next| next.tied);
    let hopo_origin = effects.hammer && next.is_some_and(|next| !next.tied);
    let hopo_destination = !note.tied && before.is_some_and(|before| before.effects.hammer);

    xml.open_with("Note", &[("id", id.to_string())]);
    xml.open("Properties");
    xml.property(
        "String",
        "String",
        Some((strings - 1 - note.string_index.min(strings - 1)).to_string()),
    );
    xml.property("Fret", "Fret", Some(note.fret.to_string()));
    xml.property("Midi", "Number", Some(pitch.to_string()));
    if effects.dead {
        xml.property("Muted", "Enable", None);
    }
    if effects.palm_mute {
        xml.property("PalmMuted", "Enable", None);
    }
    if hopo_origin {
        xml.property("HopoOrigin", "Enable", None);
    }
    if hopo_destination {
        xml.property("HopoDestination", "Enable", None);
    }
    if effects.tapping {
        xml.property("Tapped", "Enable", None);
    }
    if effects.slide {
        xml.property("Slide", "Flags", Some("1".to_owned()));
    }
    if let Some(bend) = &effects.bend {
        write_pitch_properties(xml, "Bend", "Bended", bend);
    }
    xml.close("Properties");
    if effects.ghost {
        xml.text("AntiAccent", "normal");
    }
    if effects.let_ring {
        xml.empty("LetRing");
    }
    let accent = u8::from(effects.staccato)
        | if effects.heavy_accent {
            0x04
        } else if effects.accent {
            0x08
        } else {
            0
        };
    if accent > 0 {
        xml.text("Accent", accent);
    }
    if tie_origin || note.tied {
        xml.empty_with(
            "Tie",
            &[
                ("origin", tie_origin.to_string()),
                ("destination", note.tied.to_string()),
            ],
        );
    }
    if effects.vibrato {
        xml.text("Vibrato", "Slight");
    }
    xml.close("Note");
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;
    use crate::core::{TICKS_PER_QUARTER as Q, Tick, TempoPoint};
    use crate::core::project::{BarMark, TempoTransition};

    fn single_track(notes: &[(Tick, Tick, usize, u8)]) -> BwxProject {
        let mut project = BwxProject::demo();
        project.tracks.truncate(1);
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        for &(start, length, string, fret) in notes {
            project.insert_note(id, start, length, string, fret).unwrap();
        }
        project
    }

    fn read_entry(file: &[u8], name: &str) -> String {
        let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
        let mut text = String::new();
        archive.by_name(name).unwrap().read_to_string(&mut text).unwrap();
        text
    }

    fn gpif_of(project: &BwxProject) -> String {
        read_entry(&write_gp(project).unwrap(), "Content/score.gpif")
    }

    /// Every tag closes in order, and nothing outside CDATA breaks the markup.
    fn assert_well_formed(xml: &str) {
        let mut stack: Vec<String> = Vec::new();
        let mut rest = xml;
        while let Some(start) = rest.find('<') {
            rest = &rest[start..];
            if let Some(body) = rest.strip_prefix("<![CDATA[") {
                let end = body.find("]]>").expect("unterminated CDATA");
                rest = &body[end + 3..];
                continue;
            }
            let end = rest.find('>').expect("unterminated tag");
            let tag = &rest[1..end];
            rest = &rest[end + 1..];
            if tag.starts_with('?') {
                continue;
            }
            if let Some(name) = tag.strip_prefix('/') {
                assert_eq!(stack.pop().as_deref(), Some(name), "mismatched </{name}>");
            } else if !tag.ends_with('/') {
                let name = tag.split_whitespace().next().unwrap();
                stack.push(name.to_owned());
            }
        }
        assert!(stack.is_empty(), "unclosed tags: {stack:?}");
    }

    fn count(xml: &str, pattern: &str) -> usize {
        xml.matches(pattern).count()
    }

    #[test]
    fn the_file_is_a_zip_with_the_version_and_score() {
        let file = write_gp(&single_track(&[(0, Q, 5, 0)])).unwrap();
        assert_eq!(read_entry(&file, "VERSION"), "7.0");
        let xml = read_entry(&file, "Content/score.gpif");
        assert!(xml.starts_with("<?xml"));
        assert_well_formed(&xml);
    }

    #[test]
    fn the_demo_is_well_formed_with_every_section() {
        let xml = gpif_of(&score::aligned_demo());
        assert_well_formed(&xml);
        for tag in ["<Score>", "<MasterTrack>", "<Tracks>\n", "<MasterBars>", "<Bars>\n", "<Voices>\n", "<Beats>\n", "<Notes>\n", "<Rhythms>"] {
            assert_eq!(count(&xml, tag), 1, "{tag:?}");
        }
    }

    #[test]
    fn one_bar_per_track_hangs_off_every_master_bar() {
        let project = score::aligned_demo();
        let song = score::prepare(&project).unwrap();
        let xml = gpif(&song);
        assert_eq!(count(&xml, "<MasterBar>"), song.grid.len());
        assert_eq!(count(&xml, "<Bar id="), song.grid.len() * project.tracks.len());
        assert_eq!(count(&xml, "<Track id="), project.tracks.len());
    }

    #[test]
    fn strings_count_from_the_lowest_and_tunings_list_lowest_first() {
        let project = single_track(&[(0, Q, 5, 3), (Q, Q, 0, 1)]);
        let tuning = &project.tracks[0].tuning;
        let xml = gpif_of(&project);
        let lowest_first: Vec<String> = tuning.iter().rev().map(u8::to_string).collect();
        assert!(xml.contains(&format!("<Pitches>{}</Pitches>", lowest_first.join(" "))));
        // The lowest string (last in the tuning) is string 0.
        assert!(xml.contains("<Property name=\"String\"><String>0</String></Property>"));
        let top = tuning.len() - 1;
        assert!(xml.contains(&format!("<Property name=\"String\"><String>{top}</String></Property>")));
        assert!(xml.contains(&format!("<Property name=\"Midi\"><Number>{}</Number></Property>", tuning[5] + 3)));
    }

    #[test]
    fn a_beat_lists_its_notes_and_a_rest_lists_none() {
        let xml = gpif_of(&single_track(&[(0, Q, 5, 0), (0, Q, 4, 2), (Q * 2, Q, 3, 0)]));
        assert_eq!(count(&xml, "<Note id="), 3);
        assert!(xml.contains("<Notes>0 1</Notes>"));
        assert!(xml.contains("<Notes>2</Notes>"));
        // Beat 1 is the rest between the chord and the last note.
        assert!(count(&xml, "<Beat id=") > count(&xml, "<Notes>"));
    }

    #[test]
    fn rhythms_are_shared_and_dotted_lengths_carry_a_dot() {
        let xml = gpif_of(&single_track(&[(0, Q * 3 / 2, 5, 0), (Q * 3 / 2, Q / 2, 5, 2), (Q * 2, Q, 5, 3), (Q * 3, Q, 5, 5)]));
        // Plain and dotted quarters are two rhythms; repeats of one share it.
        assert_eq!(count(&xml, "<NoteValue>Quarter</NoteValue>"), 2);
        assert!(xml.contains("<AugmentationDot count=\"1\"/>"));
        assert!(xml.contains("<NoteValue>Eighth</NoteValue>"));
    }

    #[test]
    fn a_tuplet_is_a_rhythm_with_a_primary_tuplet() {
        let xml = gpif_of(&score::tuplet_demo());
        assert_well_formed(&xml);
        // One shared rhythm per note value and tuplet: the sextuplet has a
        // half note and an eighth, the others only eighths.
        for (num, den, rhythms) in [(3, 2, 1), (5, 4, 1), (6, 4, 2)] {
            let tag = format!("<PrimaryTuplet num=\"{num}\" den=\"{den}\"/>");
            assert_eq!(count(&xml, &tag), rhythms, "{tag}");
        }
        // The triplet's notes are eighths, not 64ths snapped to the grid.
        let flat: String = xml.split_whitespace().collect::<Vec<_>>().join("");
        assert!(flat.contains("<NoteValue>Eighth</NoteValue><PrimaryTuplet"), "{xml}");
    }

    #[test]
    fn a_note_across_a_barline_is_tied_origin_to_destination() {
        let xml = gpif_of(&single_track(&[(Q * 3, Q * 2, 5, 0)]));
        assert!(xml.contains("<Tie origin=\"true\" destination=\"false\"/>"));
        assert!(xml.contains("<Tie origin=\"false\" destination=\"true\"/>"));
    }

    #[test]
    fn repeat_signs_become_a_repeat_with_the_total_plays() {
        let mut project = single_track(&[(0, Q, 5, 0)]);
        project.tracks[0].bar_marks = vec![
            BarMark { at_tick: 0, repeat_open: true, repeat_close: 0 },
            BarMark { at_tick: Q * 4, repeat_open: false, repeat_close: 2 },
        ];
        project.tracks[0].bar_count = project.tracks[0].bar_count.max(2);
        let xml = gpif_of(&project);
        assert!(xml.contains("<Repeat start=\"true\" end=\"false\"/>"));
        assert!(xml.contains("<Repeat start=\"false\" end=\"true\" count=\"3\"/>"));
    }

    #[test]
    fn tempo_changes_sit_at_their_bar_and_position() {
        let mut project = single_track(&[(0, Q, 5, 0)]);
        project.tracks[0].bar_count = 3;
        project.tempo_points = vec![
            TempoPoint { at_tick: 0, bpm: 100.0, transition: TempoTransition::Constant, hidden: false },
            TempoPoint { at_tick: Q * 4 + Q * 2, bpm: 140.0, transition: TempoTransition::Constant, hidden: false },
        ];
        let xml = gpif_of(&project);
        assert!(xml.contains("<Value>100 2</Value>"));
        assert!(xml.contains("<Bar>1</Bar>"));
        assert!(xml.contains("<Position>0.5</Position>"));
        assert!(xml.contains("<Value>140 2</Value>"));
    }

    #[test]
    fn a_progressive_point_glides_to_the_next() {
        let mut project = single_track(&[(0, Q, 5, 0)]);
        project.tempo_points = vec![
            TempoPoint { at_tick: 0, bpm: 100.0, transition: TempoTransition::Progressive, hidden: false },
            TempoPoint { at_tick: Q * 4, bpm: 140.0, transition: TempoTransition::Constant, hidden: false },
        ];
        let xml = gpif_of(&project);
        assert!(xml.contains("<Linear>true</Linear>"));
        assert!(xml.contains("<Linear>false</Linear>"));
    }

    #[test]
    fn effects_become_note_properties() {
        let mut project = single_track(&[(0, Q, 5, 0), (Q, Q, 5, 2)]);
        project.tracks[0].notes[0].effects.palm_mute = true;
        project.tracks[0].notes[0].effects.hammer = true;
        project.tracks[0].notes[1].effects.dead = true;
        project.tracks[0].notes[1].effects.ghost = true;
        let xml = gpif_of(&project);
        assert!(xml.contains("<Property name=\"PalmMuted\"><Enable/></Property>"));
        assert!(xml.contains("<Property name=\"HopoOrigin\"><Enable/></Property>"));
        assert!(xml.contains("<Property name=\"HopoDestination\"><Enable/></Property>"));
        assert!(xml.contains("<Property name=\"Muted\"><Enable/></Property>"));
        assert!(xml.contains("<AntiAccent>normal</AntiAccent>"));
    }

    #[test]
    fn titles_and_names_are_escaped() {
        let mut project = single_track(&[(0, Q, 5, 0)]);
        project.title = "Fish & <Chips> ]]> \"quoted\"".to_owned();
        project.tracks[0].name = "A&B".to_owned();
        let xml = gpif_of(&project);
        assert_well_formed(&xml);
        assert!(xml.contains("Fish & <Chips> ]]]]><![CDATA[> \"quoted\""));
    }

    #[test]
    fn the_sound_carries_bank_and_program() {
        let mut project = single_track(&[(0, Q, 5, 0)]);
        project.tracks[0].sound_auto = false;
        project.tracks[0].program = 29;
        project.tracks[0].bank = 0;
        let xml = gpif_of(&project);
        assert!(xml.contains("<Program>29</Program>"));
        assert!(xml.contains("Midi/29"));
        assert!(xml.contains("<Type>electricGuitar</Type>"));
    }

    #[test]
    fn incompatible_projects_are_refused() {
        let project = BwxProject::demo();
        assert!(matches!(write_gp(&project), Err(FormatError::Incompatible(_))));
    }
}
