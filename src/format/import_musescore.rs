//! MuseScore import: `.mscz` (a zip around the score) and bare `.mscx`.
//!
//! Each staff of each part becomes a track. A staff that says which string
//! and fret a note is on keeps them; any other has its pitches placed on the
//! strings of a guitar or bass.

use std::collections::HashSet;
use std::io::{Cursor, Read};

use crate::core::{NoteEffects, TICKS_PER_QUARTER, Tick, project::TimeSignature, tuplets::TupletBeat};

use super::{
    FormatError,
    import::{
        Import, ImportedBar, ImportedNote, ImportedSong, ImportedTrack, Report, Unplaced, bar_starts,
        place_notes, push_tempo, scaled, value_ticks,
    },
    xml::{self, Node},
};

const WHOLE: Tick = TICKS_PER_QUARTER * 4;

/// Chord children the project has no place for.
const UNSUPPORTED_CHORD: [(&str, &str); 3] = [
    ("Lyrics", "lyrics"),
    ("Arpeggio", "strums and arpeggios"),
    ("Tremolo", "tremolo picking"),
];

/// Note children the project has no place for.
const UNSUPPORTED_NOTE: [(&str, &str); 2] = [("Bend", "bends"), ("Fingering", "fingerings")];

/// Voice-level elements the project has no place for.
const UNSUPPORTED_VOICE: [(&str, &str); 8] = [
    ("Dynamic", "dynamics"),
    ("Harmony", "chord symbols"),
    ("FretDiagram", "chord diagrams"),
    ("RehearsalMark", "section markers"),
    ("Marker", "jump and coda marks"),
    ("Jump", "jump and coda marks"),
    ("Fermata", "fermatas"),
    ("Tempo", "tempo"),
];

pub fn import_mscz(bytes: &[u8]) -> Result<Import, FormatError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let name = (0..archive.len())
        .filter_map(|index| archive.name_for_index(index).map(str::to_owned))
        .find(|name| name.ends_with(".mscx") && !name.starts_with("Excerpts/"))
        .ok_or_else(|| FormatError::UnsupportedFormat("no score in the MuseScore file".to_owned()))?;
    let mut text = String::new();
    archive.by_name(&name)?.read_to_string(&mut text)?;
    import_score(&text)
}

pub fn import_mscx(bytes: &[u8]) -> Result<Import, FormatError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| FormatError::UnsupportedFormat("the score is not UTF-8 text".to_owned()))?;
    import_score(text)
}

/// A fraction of a whole note like `3/8`, in ticks.
fn fraction_ticks(text: &str) -> Option<Tick> {
    let (numerator, denominator) = text.trim().split_once('/')?;
    let (numerator, denominator): (Tick, Tick) = (numerator.trim().parse().ok()?, denominator.trim().parse().ok()?);
    (denominator != 0).then(|| numerator * WHOLE / denominator)
}

fn duration_value(name: &str) -> Option<u32> {
    Some(match name {
        "whole" => 1,
        "half" => 2,
        "quarter" => 4,
        "eighth" => 8,
        "16th" => 16,
        "32nd" => 32,
        "64th" => 64,
        "128th" => 128,
        _ => return None,
    })
}

struct Staff<'a> {
    name: String,
    program: u8,
    volume: u8,
    pan: i8,
    tuning: Option<Vec<u8>>,
    fret_count: u8,
    percussion: bool,
    content: Option<&'a Node>,
}

fn import_score(text: &str) -> Result<Import, FormatError> {
    let root = xml::parse(text)?;
    let Some(score) = root.child("Score").filter(|_| root.name == "museScore") else {
        return Err(FormatError::UnsupportedFormat("not a MuseScore score".to_owned()));
    };
    let mut song = ImportedSong {
        title: title_of(score),
        ..ImportedSong::default()
    };

    let mut staves: Vec<Staff> = Vec::new();
    for part in score.all("Part") {
        let part_staves: Vec<&Node> = part.all("Staff").collect();
        let instrument = part.child("Instrument");
        let name = part
            .text_of("trackName")
            .or_else(|| instrument.and_then(|i| i.text_of("trackName")))
            .or_else(|| instrument.and_then(|i| i.text_of("longName")))
            .filter(|name| !name.is_empty())
            .unwrap_or("Track")
            .to_owned();
        let channel = instrument.and_then(|i| i.child("Channel"));
        let program: u8 = channel
            .and_then(|c| c.child("program"))
            .and_then(|p| p.attribute("value"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(25);
        let controller = |ctrl: &str| -> Option<u8> {
            channel?
                .all("controller")
                .find(|c| c.attribute("ctrl") == Some(ctrl))?
                .attribute("value")?
                .parse()
                .ok()
        };
        let strings: Vec<u8> = instrument
            .and_then(|i| i.child("StringData"))
            .map(|data| data.all("string").filter_map(|s| s.text.trim().parse().ok()).collect())
            .unwrap_or_default();
        let tuning = (!strings.is_empty()).then(|| {
            // Highest string first.
            let mut tuning = strings.clone();
            if tuning.first() < tuning.last() {
                tuning.reverse();
            }
            tuning
        });
        let fret_count: u8 = instrument
            .and_then(|i| i.child("StringData"))
            .and_then(|data| data.number_of("frets"))
            .unwrap_or(24);
        let percussion = instrument.is_some_and(|i| i.child("Drum").is_some())
            || part_staves.iter().any(|staff| {
                staff
                    .child("StaffType")
                    .is_some_and(|kind| kind.attribute("group") == Some("percussion"))
            });
        for (position, part_staff) in part_staves.iter().enumerate() {
            let id = part_staff.attribute("id").unwrap_or("");
            staves.push(Staff {
                name: if part_staves.len() > 1 {
                    format!("{name} ({})", position + 1)
                } else {
                    name.clone()
                },
                program: program.min(127),
                volume: controller("7").unwrap_or(100),
                pan: controller("10").map_or(0, |pan| (i16::from(pan) - 64).clamp(-64, 63) as i8),
                tuning: tuning.clone(),
                fret_count,
                percussion,
                content: score.all("Staff").find(|staff| staff.attribute("id") == Some(id)),
            });
        }
    }
    if staves.is_empty() {
        return Err(FormatError::UnsupportedFormat("the score has no staves".to_owned()));
    }

    song.bars = bars_of(staves.iter().find_map(|staff| staff.content), &mut song.report);
    let starts = bar_starts(&song.bars);
    let mut tempos: Vec<(Tick, f32)> = Vec::new();
    for (index, staff) in staves.iter().enumerate() {
        let mut track = ImportedTrack::new(staff.name.clone(), staff.program);
        track.volume = staff.volume;
        track.pan = staff.pan;
        track.fret_count = staff.fret_count;
        if let Some(tuning) = &staff.tuning {
            track.tuning = tuning.clone();
        }
        if staff.percussion {
            track.percussion = true;
            track.bank = 128;
            track.name.push_str(" (percussion, not imported)");
            song.report.skip("percussion tracks (kept as empty tracks)");
        } else if let Some(content) = staff.content {
            let mut pending: Vec<Unplaced> = Vec::new();
            let mut walker = Walker {
                texts: &mut track.texts,
                tuning: &track.tuning,
                report: &mut song.report,
                notes: &mut pending,
                tempos: (index == 0).then_some(&mut tempos),
                active: HashSet::new(),
                tuplets: Vec::new(),
                tuplet_beats: &mut track.tuplet_beats,
            };
            for (bar, measure) in content.all("Measure").enumerate() {
                let (Some(&start), Some(grid)) = (starts.get(bar), song.bars.get(bar)) else {
                    walker.report.skip("measures past the end of the first staff");
                    continue;
                };
                walker.measure(measure, start, grid.signature.measure_ticks());
            }
            track.notes = place_notes(pending, &track.tuning, track.fret_count, &mut song.report);
        }
        song.tracks.push(track);
    }

    tempos.sort_by(|a, b| a.0.cmp(&b.0));
    for (tick, bpm) in tempos {
        push_tempo(&mut song.tempos, tick, bpm, false);
    }
    let mut import = song.into_import();
    if let Some(settings) = score
        .all("metaTag")
        .find(|tag| tag.attribute("name") == Some(super::extras::PROPERTY))
    {
        super::extras::apply(&mut import.project, settings.text.trim());
    }
    Ok(import)
}

fn title_of(score: &Node) -> String {
    let tag = |name: &str| {
        score
            .all("metaTag")
            .find(|tag| tag.attribute("name") == Some(name))
            .map(|tag| tag.text.trim())
            .filter(|text| !text.is_empty())
    };
    tag("workTitle")
        .or_else(|| tag("movementTitle"))
        .or_else(|| {
            score
                .all("Staff")
                .flat_map(|staff| staff.all("VBox"))
                .flat_map(|frame| frame.all("Text"))
                .find(|text| text.text_of("style") == Some("Title"))
                .and_then(|text| text.text_of("text"))
                .filter(|text| !text.is_empty())
        })
        .unwrap_or("Imported MuseScore score")
        .to_owned()
}

/// The bars of the score, read from one staff: its time signatures, bar
/// lengths and repeat signs.
fn bars_of(staff: Option<&Node>, report: &mut Report) -> Vec<ImportedBar> {
    let mut bars = Vec::new();
    let mut signature = TimeSignature::new(4, 4);
    let Some(staff) = staff else { return bars };
    for measure in staff.all("Measure") {
        let change = measure
            .all("voice")
            .flat_map(|voice| voice.all("TimeSig"))
            .next();
        if let Some(change) = change {
            let numerator: u8 = change.number_of("sigN").unwrap_or(4);
            let denominator: u8 = change.number_of("sigD").unwrap_or(4);
            signature = TimeSignature::new(numerator.max(1), denominator.max(1));
        }
        // A bar shorter or longer than the signature (a pickup) is imported
        // as a bar of its own length.
        let mut shown = signature;
        if let Some((n, d)) = measure.attribute("len").and_then(|len| len.split_once('/'))
            && let (Ok(n), Ok(d)) = (n.trim().parse::<u8>(), d.trim().parse::<u8>())
            && d != 0
            && (n, d) != (signature.numerator, signature.denominator)
            && n as Tick * signature.denominator as Tick != d as Tick * signature.numerator as Tick
        {
            shown = TimeSignature::new(n, d);
            report.skip("bars of a different length than the time signature (imported as bars of that length)");
        }
        let plays: u16 = measure.number_of("endRepeat").unwrap_or(2);
        bars.push(ImportedBar {
            signature: shown,
            repeat_open: measure.child("startRepeat").is_some(),
            repeat_close: if measure.child("endRepeat").is_some() {
                plays.saturating_sub(1).max(1)
            } else {
                0
            },
        });
    }
    bars
}

struct Walker<'a> {
    tuning: &'a [u8],
    report: &'a mut Report,
    notes: &'a mut Vec<Unplaced>,
    /// Where tempo marks go; only the first staff is asked.
    tempos: Option<&'a mut Vec<(Tick, f32)>>,
    /// Effects held by a line over the notes, e.g. "PalmMute".
    active: HashSet<String>,
    /// (normal, actual) of the tuplets open.
    tuplets: Vec<(Tick, Tick)>,
    /// The beats of the first voice that sit in a tuplet.
    tuplet_beats: &'a mut Vec<TupletBeat>,
    /// Texts above the staff; text for all staves goes to the first only.
    texts: &'a mut Vec<(Tick, String)>,
}

/// The words of a text element, which may hold markup (`<b>`, `<i>`) as
/// child elements; a literal `<` inside the words arrives already decoded.
fn plain_text(node: &Node) -> String {
    fn collect(node: &Node, into: &mut String) {
        into.push_str(&node.text);
        for child in &node.children {
            collect(child, into);
        }
    }
    let mut text = String::new();
    collect(node, &mut text);
    text.trim().to_owned()
}

impl Walker<'_> {
    fn measure(&mut self, measure: &Node, start: Tick, length: Tick) {
        self.tuplets.clear();
        for (index, voice) in measure.all("voice").enumerate() {
            self.voice(voice, index, start, length);
        }
    }

    fn voice(&mut self, voice: &Node, index: usize, start: Tick, length: Tick) {
        let mut cursor = start;
        for element in &voice.children {
            match element.name.as_str() {
                "Tuplet" => {
                    let normal: Tick = element.number_of("normalNotes").unwrap_or(1);
                    let actual: Tick = element.number_of("actualNotes").unwrap_or(1);
                    self.tuplets.push((normal.max(1), actual.max(1)));
                }
                "endTuplet" => {
                    self.tuplets.pop();
                }
                "location" => {
                    if let Some(shift) = element.text_of("fractions").and_then(fraction_ticks) {
                        cursor += shift;
                    }
                }
                "Rest" => {
                    let ticks = self.rest_ticks(element, length);
                    self.note_tuplet_beat(element, cursor, ticks, index);
                    cursor += ticks;
                }
                "Chord" => {
                    let ticks = self.duration(element);
                    self.note_tuplet_beat(element, cursor, ticks, index);
                    if element.children.iter().any(|child| is_grace(&child.name)) {
                        self.report.skip("grace notes (left out)");
                    } else {
                        self.chord(element, cursor, ticks, index);
                    }
                    cursor += ticks;
                }
                "Spanner" => self.spanner(element),
                "KeySig" => {
                    if element.number_of::<i32>("accidental").unwrap_or(0) != 0 {
                        self.report.skip("key signature changes");
                    }
                }
                "StaffText" | "SystemText" => {
                    let first_staff = self.tempos.is_some();
                    if element.name == "StaffText" || first_staff {
                        let text = element.child("text").map(plain_text).unwrap_or_default();
                        if !text.is_empty() {
                            self.texts.push((cursor, text));
                        }
                    }
                }
                "Tempo" => {
                    if let (Some(tempos), Some(per_second)) =
                        (self.tempos.as_mut(), element.number_of::<f32>("tempo"))
                    {
                        tempos.push((cursor, per_second * 60.0));
                    }
                }
                name => {
                    if let Some((_, what)) = UNSUPPORTED_VOICE.iter().find(|(tag, _)| *tag == name) {
                        self.report.skip(what);
                    }
                }
            }
        }
    }

    /// Remembers a beat that sits in one tuplet, for the spans of the track.
    /// Tuplets inside tuplets are not kept as such.
    fn note_tuplet_beat(&mut self, element: &Node, start: Tick, ticks: Tick, voice: usize) {
        let [(normal, actual)] = self.tuplets[..] else {
            return;
        };
        let (Ok(times), Ok(enters)) = (u8::try_from(normal), u8::try_from(actual)) else {
            return;
        };
        if voice != 0 || element.text_of("durationType") == Some("measure") {
            return;
        }
        let dots: u8 = element.number_of("dots").unwrap_or(0);
        let value = element.text_of("durationType").and_then(duration_value).unwrap_or(4);
        self.tuplet_beats.push(TupletBeat {
            start,
            ticks,
            written: value_ticks(value, dots),
            enters,
            times,
        });
    }

    fn duration(&self, element: &Node) -> Tick {
        let dots: u8 = element.number_of("dots").unwrap_or(0);
        let value = element.text_of("durationType").and_then(duration_value).unwrap_or(4);
        self.tuplets
            .iter()
            .fold(value_ticks(value, dots), |ticks, &(normal, actual)| scaled(ticks, normal, actual))
    }

    fn rest_ticks(&self, rest: &Node, length: Tick) -> Tick {
        if rest.text_of("durationType") == Some("measure") {
            return rest.text_of("duration").and_then(fraction_ticks).unwrap_or(length);
        }
        self.duration(rest)
    }

    fn spanner(&mut self, spanner: &Node) {
        let kind = spanner.attribute("type").unwrap_or("");
        match kind {
            "PalmMute" | "LetRing" | "Vibrato" => {
                if spanner.child("next").is_some() {
                    self.active.insert(kind.to_owned());
                } else if spanner.child("prev").is_some() {
                    self.active.remove(kind);
                }
            }
            "Tie" | "Slide" | "Glissando" | "Beam" => {}
            "Slur" => self.report.skip("slurs"),
            "HairPin" => self.report.skip("hairpins"),
            "Trill" => self.report.skip("trills"),
            _ => self.report.skip("lines and spanners (ottavas, pedals, text lines)"),
        }
    }

    fn chord(&mut self, chord: &Node, start: Tick, duration: Tick, voice: usize) {
        for (name, what) in UNSUPPORTED_CHORD {
            if chord.child(name).is_some() {
                self.report.skip(what);
            }
        }
        let mut staccato = false;
        let mut accent = false;
        let mut heavy = false;
        for articulation in chord.all("Articulation") {
            let kind = articulation.text_of("subtype").unwrap_or("");
            if kind.contains("Staccato") {
                staccato = true;
            } else if kind.contains("Marcato") {
                heavy = true;
            } else if kind.starts_with("articAccent") {
                accent = true;
            } else {
                self.report.skip("articulations (other than staccato and accents)");
            }
        }
        for note in chord.all("Note") {
            for (name, what) in UNSUPPORTED_NOTE {
                if note.child(name).is_some() {
                    self.report.skip(what);
                }
            }
            let Some(pitch) = note.number_of::<u8>("pitch") else { continue };
            let tied = note
                .all("Spanner")
                .any(|spanner| spanner.attribute("type") == Some("Tie") && spanner.child("prev").is_some());
            let mut effects = NoteEffects {
                dead: note.text_of("headGroup") == Some("cross"),
                palm_mute: self.active.contains("PalmMute"),
                let_ring: self.active.contains("LetRing"),
                vibrato: self.active.contains("Vibrato"),
                staccato,
                accent,
                heavy_accent: heavy,
                ..NoteEffects::default()
            };
            effects.slide = note.all("Spanner").any(|s| {
                matches!(s.attribute("type"), Some("Slide" | "Glissando")) && s.child("next").is_some()
            });
            let placed = self.on_strings(note, pitch);
            let imported = ImportedNote {
                start,
                duration,
                string_index: placed.map_or(0, |(string, _)| string),
                fret: placed.map_or(0, |(_, fret)| fret),
                velocity: 90,
                effects,
                tied,
            };
            let key = placed.is_none().then_some(pitch);
            if voice > 0 {
                self.report.skip("notes of a later voice merged onto the first voice's staff");
            }
            self.notes.push((imported, key));
        }
    }

    /// The string and fret the note names when they agree with its pitch,
    /// counting the strings from the lowest as the exporter does or from the
    /// highest.
    fn on_strings(&self, note: &Node, pitch: u8) -> Option<(usize, u8)> {
        let string: usize = note.number_of("string")?;
        let fret: u8 = note.number_of("fret")?;
        let count = self.tuning.len();
        [count.checked_sub(1 + string), (string < count).then_some(string)]
            .into_iter()
            .flatten()
            .find(|&index| u16::from(self.tuning[index]) + u16::from(fret) == u16::from(pitch))
            .map(|index| (index, fret))
    }
}

fn is_grace(name: &str) -> bool {
    name.starts_with("grace") || name == "acciaccatura" || name == "appoggiatura"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{BwxProject, TICKS_PER_QUARTER as Q};
    use crate::format::{musescore::{write_mscx, write_mscz}, score};

    fn round_trip(project: &BwxProject) -> BwxProject {
        import_mscz(&write_mscz(project).unwrap()).unwrap().project
    }

    #[test]
    fn the_exported_demo_comes_back_with_its_notes_bars_and_tempo() {
        let project = score::aligned_demo();
        let back = round_trip(&project);
        assert_eq!(back.title, project.title);
        assert_eq!(back.tracks.len(), project.tracks.len());
        for (a, b) in project.tracks.iter().zip(&back.tracks) {
            assert_eq!(a.tuning, b.tuning, "tuning of {}", a.name);
            assert_eq!(a.bar_count, b.bar_count);
            let key = |t: &crate::core::project::InstrumentTrack| -> Vec<(Tick, usize, u8)> {
                t.notes
                    .iter()
                    .filter(|n| !t.is_tied_continuation(n))
                    .map(|n| (n.abs_tick, n.string_index, n.fret))
                    .collect()
            };
            assert_eq!(key(a), key(b), "notes of {}", a.name);
        }
        assert_eq!(back.tempo_points[0].bpm, project.tempo_points[0].bpm);
    }

    #[test]
    fn palm_mutes_let_ring_and_the_automatic_sound_survive_a_round_trip() {
        let mut project = score::aligned_demo();
        project.tracks.truncate(1);
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        let next_bar = project.tracks[0].bar_spans()[1].start;
        for tick in [0, Q, Q * 2, Q * 3, next_bar, next_bar + Q] {
            project.insert_note(id, tick, Q, 5, 3).unwrap();
        }
        let track = project.track_mut(id).unwrap();
        let ticks = [Q, Q * 2, next_bar, next_bar + Q];
        for note in &mut track.notes {
            note.effects.palm_mute = ticks.contains(&note.abs_tick);
            note.effects.let_ring = note.abs_tick == 0;
        }
        track.set_instrument(crate::core::instruments::INSTRUMENTS.iter().find(|d| d.id.starts_with("electric-guitar")).unwrap());
        track.set_sound_automatic();
        let back = round_trip(&project);
        let track = &back.tracks[0];
        let at = |pick: fn(&NoteEffects) -> bool| -> Vec<Tick> {
            track.notes.iter().filter(|n| pick(&n.effects)).map(|n| n.abs_tick).collect()
        };
        assert_eq!(at(|e| e.palm_mute), ticks);
        assert_eq!(at(|e| e.let_ring), [0]);
        assert!(track.sound_auto);
        assert_eq!(track.instrument_id, project.tracks[0].instrument_id);
    }

    #[test]
    fn a_bare_score_opens_too() {
        let project = score::aligned_demo();
        let back = import_mscx(&write_mscx(&project).unwrap()).unwrap().project;
        assert_eq!(back.tracks.len(), project.tracks.len());
    }

    #[test]
    fn ties_repeats_and_signatures_come_back() {
        let mut project = score::aligned_demo();
        project.tracks.truncate(1);
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        project.insert_note(id, Q * 3, Q * 2, 5, 3).unwrap();
        project.tracks[0].set_repeat_open(0, true);
        let second = project.tracks[0].bar_spans()[1].start;
        project.tracks[0].set_repeat_close(second, 2);
        let track = &round_trip(&project).tracks[0];
        assert_eq!(track.notes.len(), 2);
        assert!(track.is_tied_continuation(&track.notes[1]));
        assert!(track.bar_mark_at(0).repeat_open);
        assert_eq!(track.bar_mark_at(second).repeat_close, 2);
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
        // The long note comes back in two tied pieces: before and in the tuplet.
        assert_eq!(
            score::note_places(&back.tracks[0]),
            [(5760, 960), (6720, 320), (7040, 320), (7360, 320)]
        );
    }

    #[test]
    fn a_pitch_only_staff_is_placed_on_the_strings_and_tuplets_keep_their_length() {
        let score = r#"<museScore version="4.40"><Score>
<metaTag name="workTitle">Etude</metaTag>
<Part><Staff id="1"/><trackName>Flute</trackName><Instrument id="flute"><Channel><program value="73"/></Channel></Instrument></Part>
<Part><Staff id="2"><StaffType group="percussion"/></Staff><trackName>Kit</trackName><Instrument id="drums"><Drum pitch="36"/></Instrument></Part>
<Staff id="1"><Measure><voice><TimeSig><sigN>3</sigN><sigD>4</sigD></TimeSig>
<Tempo><tempo>1.5</tempo></Tempo>
<Tuplet><normalNotes>2</normalNotes><actualNotes>3</actualNotes><baseNote>eighth</baseNote></Tuplet>
<Chord><durationType>eighth</durationType><Note><pitch>64</pitch></Note></Chord>
<Chord><durationType>eighth</durationType><Note><pitch>67</pitch></Note></Chord>
<Chord><durationType>eighth</durationType><Note><pitch>71</pitch></Note><Lyrics><text>la</text></Lyrics></Chord>
<endTuplet/>
<Chord><dots>1</dots><durationType>quarter</durationType><Note><pitch>60</pitch></Note></Chord>
<Dynamic><subtype>f</subtype></Dynamic>
</voice></Measure></Staff>
<Staff id="2"><Measure><voice><Rest><durationType>measure</durationType><duration>3/4</duration></Rest></voice></Measure></Staff>
</Score></museScore>"#;
        let import = import_score(score).unwrap();
        let project = &import.project;
        assert_eq!(project.title, "Etude");
        assert_eq!(project.tracks.len(), 2);
        assert_eq!(project.tempo_points[0].bpm, 90.0);
        let flute = &project.tracks[0];
        assert_eq!(flute.program, 73);
        let starts: Vec<(Tick, Tick)> = flute.notes.iter().map(|n| (n.abs_tick, n.duration_ticks)).collect();
        assert_eq!(starts, [(0, 320), (320, 320), (640, 320), (960, 1440)]);
        // E4 is the open first string, G4 its third fret.
        assert_eq!((flute.notes[0].string_index, flute.notes[0].fret), (0, 0));
        assert_eq!((flute.notes[1].string_index, flute.notes[1].fret), (0, 3));
        assert!(project.tracks[1].name.contains("percussion"));
        let report = import.report.join("; ");
        assert!(report.contains("lyrics") && report.contains("dynamics"));
    }

    #[test]
    fn a_score_that_is_not_musescore_is_refused() {
        assert!(import_mscx(b"<GPIF/>").is_err());
        assert!(import_mscz(b"nope").is_err());
    }
}
