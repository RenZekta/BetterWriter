//! Guitar Pro 7/8 import: the GPIF score inside the `.gp` zip.
//!
//! Strings are numbered from the lowest one in the file and from the highest
//! in the project; a tuning lists its pitches lowest first.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use crate::core::{NoteEffects, TICKS_PER_QUARTER, Tick, pitch::PitchEnvelope, project::TimeSignature, tuplets::TupletBeat};

use super::guitarpro::GP_QUARTER_TONE;

use super::{
    FormatError,
    import::{
        Import, ImportedBar, ImportedNote, ImportedSong, ImportedTrack, bar_starts, push_tempo,
        scaled, value_ticks,
    },
    xml::{self, Node},
};

/// Velocity of each dynamic marking, the inverse of the exporter's.
const DYNAMICS: [(&str, u8); 8] = [
    ("PPP", 15),
    ("PP", 31),
    ("P", 47),
    ("MP", 63),
    ("MF", 79),
    ("F", 95),
    ("FF", 111),
    ("FFF", 127),
];

/// Note properties the project has no place for, with how the report names them.
const UNSUPPORTED_NOTE: [(&str, &str); 9] = [
    ("HarmonicType", "harmonics"),
    ("HarmonicFret", "harmonics"),
    ("Trill", "trills"),
    ("Tremolo", "tremolo picking"),
    ("Brush", "strums and arpeggios"),
    ("Arpeggio", "strums and arpeggios"),
    ("Hopo", "hammer-on and pull-off links"),
    ("LeftFingering", "fingerings"),
    ("RightFingering", "fingerings"),
];

/// Beat elements the project has no place for.
const UNSUPPORTED_BEAT: [(&str, &str); 6] = [
    ("Chord", "chord diagrams"),
    ("GraceNotes", "grace notes (played as ordinary notes)"),
    ("Whammy", "tremolo bar dives"),
    ("Wah", "wah pedal marks"),
    ("Lyrics", "lyrics"),
    ("Tremolo", "tremolo picking"),
];

pub fn import_gp(bytes: &[u8]) -> Result<Import, FormatError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut text = String::new();
    archive
        .by_name("Content/score.gpif")
        .map_err(|_| {
            FormatError::UnsupportedFormat(
                "no Content/score.gpif in the file; only Guitar Pro 7 and 8 files can be opened"
                    .to_owned(),
            )
        })?
        .read_to_string(&mut text)?;
    import_gpif(&text)
}

fn id_map<'a>(root: &'a Node, group: &str, item: &'a str) -> HashMap<&'a str, &'a Node> {
    root.child(group)
        .into_iter()
        .flat_map(|group| group.all(item))
        .filter_map(|node| Some((node.attribute("id")?, node)))
        .collect()
}

fn ids(text: Option<&str>) -> Vec<&str> {
    text.unwrap_or("")
        .split_whitespace()
        .filter(|id| *id != "-1")
        .collect()
}

/// A `<Property name="...">` of a note, track or beat.
fn property<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
    node.child("Properties")?
        .all("Property")
        .find(|property| property.attribute("name") == Some(name))
}

/// The number in a `<Property name="..."><Float>` of a note or beat.
fn float_property(node: &Node, name: &str) -> Option<f32> {
    property(node, name)?.children.first()?.text.trim().parse().ok()
}

/// The bend of a note (`prefix` Bend, `enable` Bended) or the whammy bar of
/// a beat (`prefix` WhammyBar, `enable` WhammyBar) lasting `duration`: an
/// origin, a destination and a pitch held between them.
fn pitch_envelope(node: &Node, prefix: &str, enable: &str, duration: Tick) -> Option<PitchEnvelope> {
    if !flag(node, enable) {
        return None;
    }
    let value = |name: &str| float_property(node, &format!("{prefix}{name}"));
    let quarter = |gp: f32| (gp / GP_QUARTER_TONE).round() as i16;
    let origin = (value("OriginOffset").unwrap_or(0.0), quarter(value("OriginValue").unwrap_or(0.0)));
    let destination = (
        value("DestinationOffset").unwrap_or(100.0),
        quarter(value("DestinationValue").unwrap_or(0.0)),
    );
    let (first, second) = (value("MiddleOffset1").unwrap_or(0.0), value("MiddleOffset2").unwrap_or(0.0));
    let middle = value("MiddleValue").map(quarter).filter(|held| *held != 0).map(|held| {
        if first == 0.0 && second == 0.0 {
            (held, 50.0, 50.0)
        } else if first == 0.0 {
            (held, second, second)
        } else if second == 0.0 {
            (held, first, first)
        } else {
            (held, first, second)
        }
    });
    Some(PitchEnvelope::from_plateau(duration, origin, middle, destination))
}

fn flag(node: &Node, name: &str) -> bool {
    property(node, name).is_some_and(|property| {
        property
            .children
            .first()
            .is_none_or(|value| value.text.trim() != "false")
    })
}

fn signature_of(text: &str) -> TimeSignature {
    let mut parts = text.split('/');
    let numerator = parts.next().and_then(|n| n.trim().parse().ok()).unwrap_or(4);
    let denominator = parts.next().and_then(|n| n.trim().parse().ok()).unwrap_or(4);
    TimeSignature::new(u8::max(numerator, 1), u8::max(denominator, 1))
}

fn note_value(name: &str) -> u32 {
    match name {
        "Whole" => 1,
        "Half" => 2,
        "Quarter" => 4,
        "Eighth" => 8,
        "16th" => 16,
        "32nd" => 32,
        "64th" => 64,
        "128th" => 128,
        _ => 4,
    }
}

/// Quarter notes in the beat a tempo is given in.
fn tempo_unit(code: u32) -> f32 {
    match code {
        1 => 0.5,
        3 => 1.5,
        4 => 2.0,
        5 => 3.0,
        _ => 1.0,
    }
}

fn import_gpif(text: &str) -> Result<Import, FormatError> {
    let root = xml::parse(text)?;
    if root.name != "GPIF" {
        return Err(FormatError::UnsupportedFormat("not a Guitar Pro score".to_owned()));
    }
    let mut song = ImportedSong {
        title: root
            .child("Score")
            .and_then(|score| score.text_of("Title"))
            .filter(|title| !title.is_empty())
            .unwrap_or("Imported Guitar Pro score")
            .to_owned(),
        ..ImportedSong::default()
    };

    let master_bars: Vec<&Node> = root
        .child("MasterBars")
        .map(|bars| bars.all("MasterBar").collect())
        .unwrap_or_default();
    let mut last_key = String::from("0");
    for master in &master_bars {
        let repeat = master.child("Repeat");
        let repeat_flag = |name: &str| {
            repeat.and_then(|repeat| repeat.attribute(name)).is_some_and(|value| value == "true")
        };
        let plays: u16 = repeat
            .and_then(|repeat| repeat.attribute("count"))
            .and_then(|count| count.parse().ok())
            .unwrap_or(2);
        song.bars.push(ImportedBar {
            signature: signature_of(master.text_of("Time").unwrap_or("4/4")),
            repeat_open: repeat_flag("start"),
            repeat_close: if repeat_flag("end") { plays.saturating_sub(1).max(1) } else { 0 },
        });
        for (name, what) in [
            ("Section", "section markers"),
            ("Directions", "jump and coda marks"),
            ("AlternateEndings", "alternate endings"),
            ("Fermatas", "fermatas"),
        ] {
            if master.child(name).is_some() {
                song.report.skip(what);
            }
        }
        if master.text_of("TripletFeel").is_some_and(|feel| feel != "NoTripletFeel") {
            song.report.skip("triplet feel marks");
        }
        let key = master
            .child("Key")
            .and_then(|key| key.text_of("AccidentalCount"))
            .unwrap_or("0")
            .to_owned();
        if key != last_key {
            song.report.skip("key signature changes");
            last_key = key;
        }
    }
    let starts = bar_starts(&song.bars);

    if let Some(automations) = root.child("MasterTrack").and_then(|master| master.child("Automations")) {
        for automation in automations.all("Automation").filter(|a| a.text_of("Type") == Some("Tempo")) {
            let bar: usize = automation.number_of("Bar").unwrap_or(0);
            let position: f64 = automation.number_of("Position").unwrap_or(0.0);
            let mut value = automation.text_of("Value").unwrap_or("120").split_whitespace();
            let bpm: f32 = value.next().and_then(|v| v.parse().ok()).unwrap_or(120.0);
            let unit = tempo_unit(value.next().and_then(|v| v.parse().ok()).unwrap_or(2));
            let (Some(&start), Some(master)) = (starts.get(bar), song.bars.get(bar)) else {
                continue;
            };
            let tick = start + (position * master.signature.measure_ticks() as f64).round() as Tick;
            let linear = automation.text_of("Linear") == Some("true");
            push_tempo(&mut song.tempos, tick, bpm * unit, linear);
        }
    }

    let bar_nodes = id_map(&root, "Bars", "Bar");
    let voice_nodes = id_map(&root, "Voices", "Voice");
    let beat_nodes = id_map(&root, "Beats", "Beat");
    let note_nodes = id_map(&root, "Notes", "Note");
    let rhythm_nodes = id_map(&root, "Rhythms", "Rhythm");

    let track_nodes: HashMap<&str, &Node> = root
        .child("Tracks")
        .into_iter()
        .flat_map(|tracks| tracks.all("Track"))
        .filter_map(|track| Some((track.attribute("id")?, track)))
        .collect();
    let order: Vec<&str> = {
        let listed = ids(root.child("MasterTrack").and_then(|master| master.text_of("Tracks")));
        if listed.is_empty() {
            root.child("Tracks")
                .into_iter()
                .flat_map(|tracks| tracks.all("Track"))
                .filter_map(|track| track.attribute("id"))
                .collect()
        } else {
            listed
        }
    };

    for (track_index, id) in order.iter().enumerate() {
        let Some(node) = track_nodes.get(id) else { continue };
        let mut track = track_of(node, track_index, &mut song);
        if !track.percussion {
            let mut velocity = 79_u8;
            for (bar_index, master) in master_bars.iter().enumerate() {
                let Some(bar) = ids(master.text_of("Bars"))
                    .get(track_index)
                    .and_then(|id| bar_nodes.get(id))
                else {
                    continue;
                };
                for (voice_index, voice_id) in ids(bar.text_of("Voices")).into_iter().enumerate() {
                    let Some(voice) = voice_nodes.get(voice_id) else { continue };
                    let mut cursor = starts[bar_index];
                    for beat_id in ids(voice.text_of("Beats")) {
                        let Some(beat) = beat_nodes.get(beat_id) else { continue };
                        let rhythm = beat
                            .child("Rhythm")
                            .and_then(|rhythm| rhythm.attribute("ref"))
                            .and_then(|reference| rhythm_nodes.get(reference))
                            .map_or_else(Rhythm::quarter, |rhythm| rhythm_of(rhythm));
                        let length = rhythm.ticks;
                        if voice_index == 0 && rhythm.enters != rhythm.times {
                            track.tuplet_beats.push(TupletBeat {
                                start: cursor,
                                ticks: length,
                                written: rhythm.written,
                                enters: rhythm.enters,
                                times: rhythm.times,
                            });
                        }
                        if let Some(dynamic) = beat.text_of("Dynamic")
                            && let Some(&(_, value)) = DYNAMICS.iter().find(|(name, _)| *name == dynamic)
                        {
                            velocity = value;
                        }
                        if voice_index == 0
                            && let Some(text) = beat.text_of("FreeText").map(str::trim).filter(|text| !text.is_empty())
                        {
                            track.texts.push((cursor, text.to_owned()));
                        }
                        if voice_index == 0
                            && let Some(whammy) = pitch_envelope(beat, "WhammyBar", "WhammyBar", length)
                        {
                            track.dives.push((cursor, whammy));
                        }
                        for (name, what) in UNSUPPORTED_BEAT {
                            if beat.child(name).is_some() {
                                song.report.skip(what);
                            }
                        }
                        for note_id in ids(beat.text_of("Notes")) {
                            let Some(note) = note_nodes.get(note_id) else { continue };
                            if let Some(imported) = note_of(
                                note,
                                beat,
                                cursor,
                                length,
                                velocity,
                                track.tuning.len(),
                                &mut song.report,
                            ) {
                                if voice_index == 0 {
                                    track.notes.push(imported);
                                } else if track.notes.iter().any(|other| {
                                    other.start == imported.start
                                        && other.string_index == imported.string_index
                                }) {
                                    song.report.skip("notes of a later voice that sit on a first-voice note");
                                } else {
                                    song.report.skip("notes of a later voice merged onto the first voice's staff");
                                    track.notes.push(imported);
                                }
                            }
                        }
                        cursor += length;
                    }
                }
            }
        }
        song.tracks.push(track);
    }
    Ok(song.into_import())
}

/// A beat's length: what it lasts, what its value and dots make without the
/// tuplet, and the tuplet (`enters` notes in the time of `times`).
struct Rhythm {
    ticks: Tick,
    written: Tick,
    enters: u8,
    times: u8,
}

impl Rhythm {
    fn quarter() -> Self {
        Self { ticks: TICKS_PER_QUARTER, written: TICKS_PER_QUARTER, enters: 1, times: 1 }
    }
}

fn rhythm_of(rhythm: &Node) -> Rhythm {
    let dots: u8 = rhythm
        .child("AugmentationDot")
        .and_then(|dot| dot.attribute("count"))
        .and_then(|count| count.parse().ok())
        .unwrap_or(0);
    let written = value_ticks(note_value(rhythm.text_of("NoteValue").unwrap_or("Quarter")), dots);
    let Some(tuplet) = rhythm.child("PrimaryTuplet") else {
        return Rhythm { ticks: written, written, enters: 1, times: 1 };
    };
    let number = |name: &str| -> u8 {
        tuplet.attribute(name).and_then(|n| n.parse().ok()).unwrap_or(1).max(1)
    };
    let (enters, times) = (number("num"), number("den"));
    Rhythm {
        ticks: scaled(written, Tick::from(times), Tick::from(enters)),
        written,
        enters,
        times,
    }
}

fn track_of(node: &Node, index: usize, song: &mut ImportedSong) -> ImportedTrack {
    let staves: Vec<&Node> = node.child("Staves").map(|s| s.all("Staff").collect()).unwrap_or_default();
    if staves.len() > 1 {
        song.report.skip_many("extra staves of a track (only the first is read)", staves.len() - 1);
    }
    let sound = node.child("Sounds").and_then(|sounds| sounds.child("Sound"));
    let midi = sound.and_then(|sound| sound.child("MIDI"));
    let program: u8 = midi.and_then(|midi| midi.number_of("Program")).unwrap_or(25);
    let mut track = ImportedTrack::new(
        node.text_of("Name")
            .filter(|name| !name.is_empty())
            .map_or_else(|| format!("Track {}", index + 1), str::to_owned),
        program.min(127),
    );
    if let Some(midi) = midi {
        let lsb: u16 = midi.number_of("LSB").unwrap_or(0);
        let msb: u16 = midi.number_of("MSB").unwrap_or(0);
        track.bank = msb << 7 | lsb;
    }
    let set_type = node
        .child("InstrumentSet")
        .and_then(|set| set.text_of("Type"))
        .unwrap_or("")
        .to_ascii_lowercase();
    track.percussion = set_type.contains("drum") || set_type.contains("percussion");
    if track.percussion {
        track.name.push_str(" (percussion, not imported)");
        track.bank = 128;
        song.report.skip("percussion tracks (kept as empty tracks)");
    }

    if let Some(staff) = staves.first() {
        let capo: u8 = property(staff, "CapoFret")
            .and_then(|p| p.child("Fret"))
            .and_then(|f| f.text.trim().parse().ok())
            .unwrap_or(0);
        if let Some(pitches) = property(staff, "Tuning").and_then(|p| p.text_of("Pitches")) {
            let mut tuning: Vec<u8> = pitches
                .split_whitespace()
                .filter_map(|pitch| pitch.parse::<u8>().ok())
                .map(|pitch| pitch.saturating_add(capo).min(127))
                .collect();
            if !tuning.is_empty() {
                tuning.reverse();
                track.tuning = tuning;
            }
        }
        if let Some(frets) = property(staff, "FretCount")
            .and_then(|p| p.child("Fret"))
            .and_then(|f| f.text.trim().parse().ok())
        {
            track.fret_count = frets;
        }
    }

    let strip = node
        .child("RSE")
        .and_then(|rse| rse.child("ChannelStrip"))
        .and_then(|strip| strip.text_of("Parameters"))
        .map(|text| text.split_whitespace().filter_map(|v| v.parse::<f32>().ok()).collect::<Vec<_>>())
        .unwrap_or_default();
    if let (Some(pan), Some(volume)) = (strip.get(11), strip.get(12)) {
        track.volume = (volume * 127.0).round().clamp(0.0, 127.0) as u8;
        track.pan = (pan * 127.0 - 64.0).round().clamp(-64.0, 63.0) as i8;
    }
    match node.text_of("PlaybackState") {
        Some("Solo") => track.solo = true,
        Some("Mute") => track.mute = true,
        _ => {}
    }
    track
}

fn note_of(
    note: &Node,
    beat: &Node,
    start: Tick,
    duration: Tick,
    velocity: u8,
    strings: usize,
    report: &mut super::import::Report,
) -> Option<ImportedNote> {
    let number = |name: &str| -> Option<i64> {
        property(note, name)?.children.first()?.text.trim().parse().ok()
    };
    let from_low = usize::try_from(number("String")?).ok()?;
    let fret = u8::try_from(number("Fret")?.clamp(0, 255)).unwrap_or(0);
    for (name, what) in UNSUPPORTED_NOTE {
        if property(note, name).is_some() || note.child(name).is_some() {
            report.skip(what);
        }
    }
    let accent: u8 = note.number_of("Accent").unwrap_or(0);
    let tied = note
        .child("Tie")
        .is_some_and(|tie| tie.attribute("destination") == Some("true"));
    let effects = NoteEffects {
        dead: flag(note, "Muted"),
        palm_mute: flag(note, "PalmMuted"),
        let_ring: note.child("LetRing").is_some(),
        vibrato: note.child("Vibrato").is_some(),
        slide: property(note, "Slide").is_some(),
        hammer: flag(note, "HopoOrigin"),
        ghost: note.child("AntiAccent").is_some(),
        accent: accent & 0x08 != 0,
        heavy_accent: accent & 0x04 != 0,
        staccato: accent & 0x01 != 0,
        tapping: flag(note, "Tapped"),
        slapping: flag(beat, "Slapped"),
        popping: flag(beat, "Popped"),
        fade_in: beat.text_of("Fadding") == Some("FadeIn"),
        bend: pitch_envelope(note, "Bend", "Bended", duration),
    };
    Some(ImportedNote {
        start,
        duration,
        string_index: strings.saturating_sub(1).saturating_sub(from_low),
        fret,
        velocity,
        effects,
        tied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{BwxProject, TICKS_PER_QUARTER as Q};
    use crate::format::{guitarpro::write_gp, score};

    fn round_trip(project: &BwxProject) -> BwxProject {
        import_gp(&write_gp(project).unwrap()).unwrap().project
    }

    #[test]
    fn the_exported_demo_comes_back_with_its_notes_bars_and_tempo() {
        let project = score::aligned_demo();
        let back = round_trip(&project);
        assert_eq!(back.tracks.len(), project.tracks.len());
        for (a, b) in project.tracks.iter().zip(&back.tracks) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.tuning, b.tuning);
            assert_eq!(a.program, b.sound().1);
            let key = |t: &crate::core::project::InstrumentTrack| -> Vec<(Tick, usize, u8)> {
                t.notes
                    .iter()
                    .filter(|n| !t.is_tied_continuation(n))
                    .map(|n| (n.abs_tick, n.string_index, n.fret))
                    .collect()
            };
            assert_eq!(key(a), key(b), "notes of {}", a.name);
        }
        assert_eq!(back.title, project.title);
        assert_eq!(back.tempo_points[0].bpm, project.tempo_points[0].bpm);
    }

    #[test]
    fn effects_ties_repeats_and_signatures_survive() {
        let mut project = score::aligned_demo();
        project.tracks.truncate(1);
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        let a = project.insert_note(id, 0, Q, 5, 3).unwrap();
        project.insert_note(id, Q, Q * 3, 4, 2).unwrap();
        let effects = |project: &mut BwxProject, note: u64, set: fn(&mut NoteEffects)| {
            let track = project.track_mut(id).unwrap();
            let note = track.notes.iter_mut().find(|n| n.id == note).unwrap();
            set(&mut note.effects);
        };
        effects(&mut project, a, |e| {
            e.palm_mute = true;
            e.accent = true;
            e.dead = true;
        });
        project.tracks[0].set_repeat_open(0, true);
        let second = project.tracks[0].bar_spans()[1].start;
        project.tracks[0].set_repeat_close(second, 3);
        project.tracks[0].solo = true;
        project.tracks[0].volume = 80;
        project.tracks[0].pan = -20;
        let back = round_trip(&project);
        let track = &back.tracks[0];
        let note = track.notes.iter().find(|n| n.abs_tick == 0).unwrap();
        assert!(note.effects.palm_mute && note.effects.accent && note.effects.dead);
        assert!(track.bar_mark_at(0).repeat_open);
        assert_eq!(track.bar_mark_at(second).repeat_close, 3);
        assert!(track.solo);
        assert_eq!((track.volume, track.pan), (80, -20));
    }

    #[test]
    fn tuplets_come_back_as_spans_with_their_exact_parts() {
        let project = score::tuplet_demo();
        let back = round_trip(&project);
        assert_eq!(back.tracks[0].tuplets, project.tracks[0].tuplets);
        assert_eq!(score::note_places(&back.tracks[0]), score::note_places(&project.tracks[0]));
    }

    #[test]
    fn a_tuplet_a_note_was_stretched_into_comes_back_with_its_tied_part() {
        let (project, span) = score::stretched_tuplet_demo();
        let back = round_trip(&project);
        assert_eq!(back.tracks[0].tuplets, [span]);
        assert_eq!(
            score::note_places(&back.tracks[0]),
            [(5760, 960), (6720, 320), (7040, 320), (7360, 320)]
        );
    }

    #[test]
    fn a_note_across_the_barline_comes_back_tied() {
        let mut project = score::aligned_demo();
        project.tracks.truncate(1);
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        project.insert_note(id, Q * 3, Q * 2, 5, 3).unwrap();
        let track = &round_trip(&project).tracks[0];
        assert_eq!(track.notes.len(), 2);
        assert!(track.is_tied_continuation(&track.notes[1]));
    }

    #[test]
    fn unsupported_things_are_reported_but_the_notes_stay() {
        let score = r#"<GPIF><Score><Title>T</Title></Score>
<MasterTrack><Tracks>0 1</Tracks><Automations>
<Automation><Type>Tempo</Type><Linear>false</Linear><Bar>0</Bar><Position>0</Position><Visible>true</Visible><Value>90 4</Value></Automation>
</Automations></MasterTrack>
<Tracks><Track id="0"><Name>Gtr</Name>
<Staves><Staff><Properties><Property name="Tuning"><Pitches>40 45 50 55 59 64</Pitches></Property></Properties></Staff></Staves>
<Sounds><Sound><MIDI><Program>29</Program></MIDI></Sound></Sounds></Track>
<Track id="1"><Name>Kit</Name><InstrumentSet><Type>drumKit</Type></InstrumentSet></Track></Tracks>
<MasterBars><MasterBar><Time>3/4</Time><Bars>0 1</Bars></MasterBar></MasterBars>
<Bars><Bar id="0"><Voices>0 -1 -1 -1</Voices></Bar><Bar id="1"><Voices>-1 -1 -1 -1</Voices></Bar></Bars>
<Voices><Voice id="0"><Beats>0 1</Beats></Voice></Voices>
<Beats><Beat id="0"><Rhythm ref="0"/><Notes>0</Notes><FreeText>x</FreeText></Beat>
<Beat id="1"><Rhythm ref="1"/><Notes>1</Notes></Beat></Beats>
<Notes><Note id="0"><Properties><Property name="String"><String>5</String></Property><Property name="Fret"><Fret>7</Fret></Property><Property name="HarmonicType"><HType>Natural</HType></Property></Properties></Note>
<Note id="1"><Properties><Property name="String"><String>0</String></Property><Property name="Fret"><Fret>2</Fret></Property></Properties></Note></Notes>
<Rhythms><Rhythm id="0"><NoteValue>Quarter</NoteValue></Rhythm>
<Rhythm id="1"><NoteValue>Eighth</NoteValue><PrimaryTuplet num="3" den="2"/></Rhythm></Rhythms></GPIF>"#;
        let import = import_gpif(score).unwrap();
        let project = &import.project;
        assert_eq!(project.tracks.len(), 2);
        let guitar = &project.tracks[0];
        assert_eq!(guitar.program, 29);
        assert_eq!(guitar.notes.len(), 2);
        assert_eq!((guitar.notes[0].string_index, guitar.notes[0].fret), (0, 7));
        assert_eq!((guitar.notes[1].string_index, guitar.notes[1].fret), (5, 2));
        assert_eq!(guitar.notes[1].abs_tick, Q);
        assert_eq!(guitar.notes[1].duration_ticks, 320);
        assert!(project.tracks[1].notes.is_empty());
        assert!(project.tracks[1].name.contains("percussion"));
        assert_eq!(project.tempo_points[0].bpm, 180.0);
        let report = import.report.join("; ");
        assert!(report.contains("harmonics") && !report.contains("text"));
        assert_eq!(guitar.annotations.len(), 1);
        assert_eq!(guitar.annotations[0].text, "x");
        assert!(report.contains("percussion"));
    }

    #[test]
    fn a_zip_without_a_score_is_refused() {
        assert!(import_gp(b"not a zip").is_err());
    }
}
