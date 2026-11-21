//! MuseScore 4 export: `.mscz` (a zip around the score) and bare `.mscx`.
//!
//! Every track becomes a part with one tablature staff. Strings are numbered
//! from the lowest one and the instrument lists its tuning lowest first.

use std::io::{Cursor, Write};

use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::core::{BwxProject, Tick, sounds::midi_pan};

use super::{
    FormatError,
    score::{self, ExportBeat, ExportNote, ExportSong, GridBar},
    xml::Xml,
};

const SCORE_FILE: &str = "score.mscx";

/// Writes `project` as a MuseScore 4 file inside a `.mscz` container.
pub fn write_mscz(project: &BwxProject) -> Result<Vec<u8>, FormatError> {
    let score = write_mscx(project)?;
    let mut archive = ZipWriter::new(Cursor::new(Vec::<u8>::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive.start_file("META-INF/container.xml", options)?;
    archive.write_all(
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<container>\n  <rootfiles>\n    <rootfile full-path=\"{SCORE_FILE}\"/>\n  </rootfiles>\n</container>\n"
        )
        .as_bytes(),
    )?;
    archive.start_file(SCORE_FILE, options)?;
    archive.write_all(&score)?;
    Ok(archive.finish()?.into_inner())
}

/// Writes `project` as an uncompressed MuseScore 4 score.
pub fn write_mscx(project: &BwxProject) -> Result<Vec<u8>, FormatError> {
    let song = score::prepare(project)?;
    Ok(mscx(&song).into_bytes())
}

fn gcd(a: Tick, b: Tick) -> Tick {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// A span of ticks as a reduced fraction of a whole note, e.g. `3/8`.
fn fraction(ticks: Tick) -> String {
    let whole = score::QUANTUM * 64;
    let divisor = gcd(ticks.abs(), whole).max(1);
    format!("{}/{}", ticks / divisor, whole / divisor)
}

fn duration_type(value: u32) -> &'static str {
    match value {
        1 => "whole",
        2 => "half",
        4 => "quarter",
        8 => "eighth",
        16 => "16th",
        32 => "32nd",
        _ => "64th",
    }
}

/// Tonal pitch class (line of fifths, C = 14) of a MIDI key, spelled with
/// flats for the black keys between D and A.
fn tpc(key: u8) -> u8 {
    const BY_PITCH_CLASS: [u8; 12] = [14, 21, 16, 11, 18, 13, 20, 15, 10, 17, 12, 19];
    BY_PITCH_CLASS[usize::from(key % 12)]
}

fn mscx(song: &ExportSong) -> String {
    let mut xml = Xml::new();
    xml.out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.open_with("museScore", &[("version", "4.40".to_owned())]);
    xml.text("programVersion", "4.4.0");
    xml.text("programRevision", "");
    xml.open("Score");
    xml.text("currentLayer", 0);
    xml.text("Division", 480);
    xml.text("showInvisible", 1);
    xml.text("showUnprintable", 1);
    xml.text("showFrames", 1);
    xml.text("showMargins", 0);
    for (name, value) in [
        ("arranger", ""),
        ("composer", ""),
        ("copyright", ""),
        ("lyricist", ""),
        ("movementNumber", ""),
        ("movementTitle", ""),
        ("source", ""),
        ("translator", ""),
        ("workNumber", ""),
        ("workTitle", song.project.title.as_str()),
    ] {
        xml.text_with("metaTag", &[("name", name.to_owned())], value);
    }

    for (index, exported) in song.tracks.iter().enumerate() {
        write_part(&mut xml, index, exported.track);
    }
    for (index, exported) in song.tracks.iter().enumerate() {
        write_staff(&mut xml, song, index, exported);
    }
    xml.close("Score");
    xml.close("museScore");
    xml.out
}

fn write_part(xml: &mut Xml, index: usize, track: &crate::core::project::InstrumentTrack) {
    let strings = track.tuning.len().max(1);
    let (_, program) = track.sound();
    let program = program.min(127);
    let (id, instrument_id) = match program {
        24 => ("guitar-nylon", "pluck.guitar.nylon-string"),
        25 => ("guitar-steel", "pluck.guitar.steel-string"),
        32..=39 => ("bass-guitar", "pluck.bass"),
        _ => ("electric-guitar", "pluck.guitar.electric"),
    };
    let channel = track.channel.min(15);
    let clef = if (32..=39).contains(&program) { "F8vb" } else { "G8vb" };

    xml.open("Part");
    xml.open_with("Staff", &[("id", (index + 1).to_string())]);
    xml.open_with("StaffType", &[("group", "tablature".to_owned())]);
    xml.text("name", format!("tab{strings}StrCommon"));
    xml.text("lines", strings);
    xml.text("lineDistance", "1.5");
    xml.text("timesig", 0);
    xml.text("durations", 0);
    xml.text("durationFontName", "MuseScore Tab Modern");
    xml.text("durationFontSize", 15);
    xml.text("durationFontY", 0);
    xml.text("fretFontName", "MuseScore Tab Serif");
    xml.text("fretFontSize", 9);
    xml.text("fretFontY", 0);
    xml.text("linesThrough", 0);
    xml.text("minimStyle", 1);
    xml.text("onLines", 1);
    xml.text("showRests", 0);
    xml.text("stemsDown", 1);
    xml.text("stemsThrough", 0);
    xml.text("upsideDown", 0);
    xml.text("useNumbers", 1);
    xml.close("StaffType");
    xml.text("defaultClef", clef);
    xml.close("Staff");
    xml.text("trackName", &track.name);

    xml.open_with("Instrument", &[("id", id.to_owned())]);
    xml.text("longName", &track.name);
    xml.text("shortName", "");
    xml.text("trackName", &track.name);
    xml.text("instrumentId", instrument_id);
    xml.text("clef", clef);
    xml.open("StringData");
    xml.text("frets", track.fret_count.max(1));
    for pitch in track.tuning.iter().rev() {
        xml.text("string", pitch);
    }
    xml.close("StringData");
    xml.open("Channel");
    xml.empty_with("program", &[("value", program.to_string())]);
    xml.empty_with("controller", &[("ctrl", "7".to_owned()), ("value", track.volume.min(127).to_string())]);
    xml.empty_with("controller", &[("ctrl", "10".to_owned()), ("value", midi_pan(track.pan).to_string())]);
    xml.text("synti", "Fluid");
    xml.text("midiPort", 0);
    xml.text("midiChannel", channel);
    xml.close("Channel");
    xml.close("Instrument");
    xml.close("Part");
}

/// How far into its measure a tick lies.
fn offset_in_bar(bar: &GridBar, tick: Tick) -> Tick {
    tick - bar.start
}

fn write_location(xml: &mut Xml, tag: &str, measures: i64, ticks: Tick) {
    xml.open(tag);
    xml.open("location");
    if measures != 0 {
        xml.text("measures", measures);
    }
    if ticks != 0 {
        xml.text("fractions", fraction(ticks));
    }
    xml.close("location");
    xml.close(tag);
}

/// A beat with the grid bar it starts in, so tie targets can be located.
struct Placed<'a> {
    bar: usize,
    beat: &'a ExportBeat,
}

fn write_staff(xml: &mut Xml, song: &ExportSong, index: usize, exported: &score::ExportTrack) {
    let strings = exported.track.tuning.len().max(1);
    let flat: Vec<Placed> = exported
        .bars
        .iter()
        .enumerate()
        .flat_map(|(bar, content)| content.beats.iter().map(move |beat| Placed { bar, beat }))
        .collect();
    let mut cursor = 0;

    xml.open_with("Staff", &[("id", (index + 1).to_string())]);
    if index == 0 {
        xml.open("VBox");
        xml.text("height", 10);
        xml.open("Text");
        xml.text("style", "title");
        xml.text("text", &song.project.title);
        xml.close("Text");
        xml.close("VBox");
    }
    let mut previous_signature = None;
    for (bar_index, bar) in exported.bars.iter().enumerate() {
        let grid = &song.grid[bar_index];
        xml.open("Measure");
        if grid.repeat_open {
            xml.empty("startRepeat");
        }
        if grid.repeat_close > 0 {
            xml.text("endRepeat", u32::from(grid.repeat_close) + 1);
        }
        xml.open("voice");
        if previous_signature != Some(grid.signature) {
            xml.open("TimeSig");
            xml.text("sigN", grid.signature.numerator);
            xml.text("sigD", grid.signature.denominator);
            xml.close("TimeSig");
            previous_signature = Some(grid.signature);
        }

        let whole_bar_rest = bar.beats.len() == 1 && bar.beats[0].notes.is_empty();
        let mut pending: Vec<(Tick, f32)> = if index == 0 {
            song.tempos
                .iter()
                .filter(|event| event.tick >= grid.start && event.tick < grid.end)
                .map(|event| (event.tick, event.bpm))
                .collect()
        } else {
            Vec::new()
        };
        pending.reverse();

        for beat in &bar.beats {
            while pending.last().is_some_and(|&(tick, _)| tick <= beat.start) {
                let (_, bpm) = pending.pop().unwrap();
                xml.open("Tempo");
                xml.text("tempo", format!("{:.5}", bpm / 60.0));
                xml.close("Tempo");
            }
            let (value, dots) = score::value_and_dots(beat.duration);
            if beat.notes.is_empty() {
                xml.open("Rest");
                if whole_bar_rest {
                    xml.text("durationType", "measure");
                    xml.text(
                        "duration",
                        format!("{}/{}", grid.signature.numerator, grid.signature.denominator),
                    );
                } else {
                    if dots > 0 {
                        xml.text("dots", dots);
                    }
                    xml.text("durationType", duration_type(value));
                }
                xml.close("Rest");
            } else {
                xml.open("Chord");
                if dots > 0 {
                    xml.text("dots", dots);
                }
                xml.text("durationType", duration_type(value));
                if beat.notes.iter().any(|note| note.effects.staccato) {
                    xml.open("Articulation");
                    xml.text("subtype", "articStaccatoAbove");
                    xml.close("Articulation");
                }
                if beat.notes.iter().any(|note| note.effects.heavy_accent) {
                    xml.open("Articulation");
                    xml.text("subtype", "articMarcatoAbove");
                    xml.close("Articulation");
                } else if beat.notes.iter().any(|note| note.effects.accent) {
                    xml.open("Articulation");
                    xml.text("subtype", "articAccentAbove");
                    xml.close("Articulation");
                }
                for note in &beat.notes {
                    let following = flat.get(cursor + 1);
                    let before = cursor.checked_sub(1).map(|at| &flat[at]);
                    write_note(
                        xml,
                        song,
                        exported.track.tuning.as_slice(),
                        strings,
                        note,
                        &flat[cursor],
                        before,
                        following,
                    );
                }
                xml.close("Chord");
            }
            cursor += 1;
        }
        xml.close("voice");
        xml.close("Measure");
    }
    xml.close("Staff");
}

#[allow(clippy::too_many_arguments)]
fn write_note(
    xml: &mut Xml,
    song: &ExportSong,
    tuning: &[u8],
    strings: usize,
    note: &ExportNote,
    here: &Placed,
    before: Option<&Placed>,
    following: Option<&Placed>,
) {
    let pitch = tuning
        .get(note.string_index)
        .copied()
        .unwrap_or(40)
        .saturating_add(note.fret)
        .min(127);
    let link = |other: Option<&Placed>| -> Option<(i64, Tick)> {
        let other = other?;
        other
            .beat
            .notes
            .iter()
            .any(|candidate| candidate.string_index == note.string_index)
            .then(|| {
                let from = offset_in_bar(&song.grid[here.bar], here.beat.start);
                let to = offset_in_bar(&song.grid[other.bar], other.beat.start);
                (other.bar as i64 - here.bar as i64, to - from)
            })
    };
    let tie_next = following
        .filter(|next| {
            next.beat
                .notes
                .iter()
                .any(|candidate| candidate.string_index == note.string_index && candidate.tied)
        })
        .and_then(|next| link(Some(next)));
    let tie_prev = if note.tied { link(before) } else { None };

    xml.open("Note");
    if let Some((measures, ticks)) = tie_next {
        xml.open_with("Spanner", &[("type", "Tie".to_owned())]);
        xml.open("Tie");
        xml.close("Tie");
        write_location(xml, "next", measures, ticks);
        xml.close("Spanner");
    }
    if let Some((measures, ticks)) = tie_prev {
        xml.open_with("Spanner", &[("type", "Tie".to_owned())]);
        write_location(xml, "prev", measures, ticks);
        xml.close("Spanner");
    }
    if note.effects.dead {
        xml.text("headGroup", "cross");
    }
    xml.text("pitch", pitch);
    xml.text("tpc", tpc(pitch));
    xml.text("fret", note.fret);
    xml.text("string", strings - 1 - note.string_index.min(strings - 1));
    xml.close("Note");
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;
    use crate::core::TICKS_PER_QUARTER as Q;
    use crate::core::project::{BarMark, TempoPoint, TempoTransition};

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

    fn score_of(project: &BwxProject) -> String {
        String::from_utf8(write_mscx(project).unwrap()).unwrap()
    }

    fn assert_well_formed(xml: &str) {
        let mut stack: Vec<String> = Vec::new();
        let mut rest = xml;
        while let Some(start) = rest.find('<') {
            rest = &rest[start..];
            let end = rest.find('>').expect("unterminated tag");
            let tag = &rest[1..end];
            rest = &rest[end + 1..];
            if tag.starts_with('?') {
                continue;
            }
            if let Some(name) = tag.strip_prefix('/') {
                assert_eq!(stack.pop().as_deref(), Some(name), "mismatched </{name}>");
            } else if !tag.ends_with('/') {
                stack.push(tag.split_whitespace().next().unwrap().to_owned());
            }
        }
        assert!(stack.is_empty(), "unclosed tags: {stack:?}");
    }

    #[test]
    fn the_container_names_the_score() {
        let file = write_mscz(&single_track(&[(0, Q, 5, 0)])).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
        let mut container = String::new();
        archive
            .by_name("META-INF/container.xml")
            .unwrap()
            .read_to_string(&mut container)
            .unwrap();
        assert!(container.contains("full-path=\"score.mscx\""));
        let mut score = String::new();
        archive.by_name("score.mscx").unwrap().read_to_string(&mut score).unwrap();
        assert!(score.contains("<museScore version=\"4.40\">"));
        assert_well_formed(&score);
    }

    #[test]
    fn the_demo_is_well_formed_with_a_part_and_staff_per_track() {
        let project = score::aligned_demo();
        let xml = score_of(&project);
        assert_well_formed(&xml);
        assert_eq!(xml.matches("<Part>").count(), project.tracks.len());
        let song = score::prepare(&project).unwrap();
        assert_eq!(
            xml.matches("<Measure>").count(),
            song.grid.len() * project.tracks.len()
        );
    }

    #[test]
    fn instruments_list_tunings_lowest_first_and_strings_count_from_the_lowest() {
        let project = single_track(&[(0, Q, 5, 3), (Q, Q, 0, 1)]);
        let tuning = project.tracks[0].tuning.clone();
        let xml = score_of(&project);
        let lowest_first: String = tuning
            .iter()
            .rev()
            .map(|pitch| format!("<string>{pitch}</string>"))
            .collect::<Vec<_>>()
            .join("\n");
        let flattened = xml
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("<string>"))
            .take(tuning.len())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(flattened, lowest_first);
        assert!(xml.contains("<fret>3</fret>"));
        assert!(xml.contains(&format!("<pitch>{}</pitch>", tuning[5] + 3)));
        // The note lines: fret 3 on the lowest string is string 0.
        let lines: Vec<&str> = xml.lines().map(str::trim).collect();
        let at = lines.iter().position(|line| *line == "<fret>3</fret>").unwrap();
        assert_eq!(lines[at + 1], "<string>0</string>");
        let at = lines.iter().position(|line| *line == "<fret>1</fret>").unwrap();
        assert_eq!(lines[at + 1], format!("<string>{}</string>", tuning.len() - 1));
    }

    #[test]
    fn durations_and_dots_follow_note_values() {
        let xml = score_of(&single_track(&[(0, Q * 3 / 2, 5, 0), (Q * 3 / 2, Q / 2, 5, 2)]));
        assert!(xml.contains("<dots>1</dots>"));
        assert!(xml.contains("<durationType>quarter</durationType>"));
        assert!(xml.contains("<durationType>eighth</durationType>"));
    }

    #[test]
    fn an_empty_bar_is_a_measure_rest() {
        let mut project = single_track(&[(0, Q, 5, 0)]);
        project.tracks[0].bar_count = 2;
        let xml = score_of(&project);
        assert!(xml.contains("<durationType>measure</durationType>"));
        assert!(xml.contains("<duration>4/4</duration>"));
    }

    #[test]
    fn a_constant_time_signature_is_written_once() {
        let xml = score_of(&single_track(&[(0, Q, 5, 0)]));
        assert_eq!(xml.matches("<TimeSig>").count(), 1);
    }

    #[test]
    fn a_note_across_a_barline_is_tied_with_a_location() {
        let xml = score_of(&single_track(&[(Q * 3, Q * 2, 5, 0)]));
        assert!(xml.contains("<Spanner type=\"Tie\">"));
        assert!(xml.contains("<measures>1</measures>"));
        assert!(xml.contains("<fractions>-3/4</fractions>"));
        assert!(xml.contains("<prev>"));
    }

    #[test]
    fn repeats_carry_the_total_plays() {
        let mut project = single_track(&[(0, Q, 5, 0)]);
        project.tracks[0].bar_marks = vec![
            BarMark { at_tick: 0, repeat_open: true, repeat_close: 0 },
            BarMark { at_tick: Q * 4, repeat_open: false, repeat_close: 2 },
        ];
        project.tracks[0].bar_count = project.tracks[0].bar_count.max(2);
        let xml = score_of(&project);
        assert!(xml.contains("<startRepeat/>"));
        assert!(xml.contains("<endRepeat>3</endRepeat>"));
    }

    #[test]
    fn tempo_is_written_in_beats_per_second_before_its_beat() {
        let mut project = single_track(&[(0, Q, 5, 0), (Q * 2, Q, 5, 2)]);
        project.tempo_points = vec![
            TempoPoint { at_tick: 0, bpm: 120.0, transition: TempoTransition::Constant, hidden: false },
            TempoPoint { at_tick: Q * 2, bpm: 60.0, transition: TempoTransition::Constant, hidden: false },
        ];
        let xml = score_of(&project);
        assert!(xml.contains("<tempo>2.00000</tempo>"));
        assert!(xml.contains("<tempo>1.00000</tempo>"));
    }

    #[test]
    fn dead_notes_use_the_cross_head() {
        let mut project = single_track(&[(0, Q, 5, 0)]);
        project.tracks[0].notes[0].effects.dead = true;
        assert!(score_of(&project).contains("<headGroup>cross</headGroup>"));
    }

    #[test]
    fn markup_in_titles_and_names_is_escaped() {
        let mut project = single_track(&[(0, Q, 5, 0)]);
        project.title = "A & <B>".to_owned();
        project.tracks[0].name = "Lead \"1\"".to_owned();
        let xml = score_of(&project);
        assert_well_formed(&xml);
        assert!(xml.contains("A &amp; &lt;B&gt;"));
    }

    #[test]
    fn fractions_reduce_to_whole_note_parts() {
        assert_eq!(fraction(Q), "1/4");
        assert_eq!(fraction(-Q * 3), "-3/4");
        assert_eq!(fraction(Q / 2), "1/8");
        assert_eq!(fraction(Q * 8), "2/1");
    }

    #[test]
    fn incompatible_projects_are_refused() {
        assert!(matches!(write_mscz(&BwxProject::demo()), Err(FormatError::Incompatible(_))));
    }
}
