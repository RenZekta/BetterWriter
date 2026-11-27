//! A project laid out the way notation formats store it: one shared bar
//! grid, and for each track the beats of every bar. The beats tile each bar
//! without gaps or overlaps, every length is a (dotted) note value, and
//! anything longer is tied across beats or barlines.

use crate::core::{
    BwxProject, CompatibilityReport, NoteEffects, TICKS_PER_QUARTER, Tick,
    pitch::PitchEnvelope,
    project::{InstrumentTrack, TempoTransition, TimeSignature},
    tuplets::TupletSpan,
};

use super::FormatError;

/// Times snap to a 64th note, the shortest length the target formats share.
pub const QUANTUM: Tick = TICKS_PER_QUARTER / 16;

/// Undotted lengths from a whole note to a 64th, longest first.
const VALUES: [Tick; 7] = [
    TICKS_PER_QUARTER * 4,
    TICKS_PER_QUARTER * 2,
    TICKS_PER_QUARTER,
    TICKS_PER_QUARTER / 2,
    TICKS_PER_QUARTER / 4,
    TICKS_PER_QUARTER / 8,
    TICKS_PER_QUARTER / 16,
];

/// A bar every track shares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridBar {
    pub start: Tick,
    pub end: Tick,
    pub signature: TimeSignature,
    pub repeat_open: bool,
    /// Extra plays of the region this bar closes; 0 when it closes none.
    pub repeat_close: u16,
}

/// One change of the shared tempo curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoEvent {
    pub tick: Tick,
    pub bpm: f32,
    /// The tempo glides to the next event's instead of jumping.
    pub linear: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportNote {
    /// Index into the track's tuning, whose first entry is the highest string.
    pub string_index: usize,
    pub fret: u8,
    pub velocity: u8,
    /// `effects.bend` holds the part of the note's bend that falls in this
    /// beat, with times counted from the beat's start.
    pub effects: NoteEffects,
    /// Where the tremolo bar goes during this beat, times counted from the
    /// beat's start. Only the first note of a beat carries it.
    pub tremolo: Option<PitchEnvelope>,
    /// Sounds on from the previous beat's note of the same string.
    pub tied: bool,
}

/// `enters` notes in the time of `times` notes of the same value: three in
/// the time of two is a triplet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tuplet {
    pub enters: u8,
    pub times: u8,
    /// What one part of the tuplet is written as, the note value its
    /// brackets count in (an eighth for eighth triplets).
    pub unit: Tick,
    /// Whether the beat opens the tuplet, or closes it. The notes between
    /// them, the rests too, belong to it.
    pub starts: bool,
    pub ends: bool,
}

/// One beat of a track; no notes makes it a rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportBeat {
    pub start: Tick,
    /// How long the beat really lasts.
    pub duration: Tick,
    /// The note value it is written as: the same as `duration` unless the
    /// beat is part of a tuplet, where it is what the value would last
    /// outside it (an eighth of a triplet lasts a third of a quarter).
    pub written: Tick,
    pub tuplet: Option<Tuplet>,
    pub notes: Vec<ExportNote>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportBar {
    pub beats: Vec<ExportBeat>,
    /// Nothing sounds in the bar.
    pub empty: bool,
}

pub struct ExportTrack<'a> {
    pub track: &'a InstrumentTrack,
    pub bars: Vec<ExportBar>,
}

impl<'a> ExportTrack<'a> {
    /// The texts written over `beat`, that is, anywhere from its start up to
    /// where the next beat begins.
    pub fn texts_in(&self, beat: &ExportBeat) -> Vec<&'a str> {
        self.track
            .annotations
            .iter()
            .filter(|note| note.tick >= beat.start && note.tick < beat.start + beat.duration)
            .map(|note| note.text.as_str())
            .collect()
    }
}

pub struct ExportSong<'a> {
    pub project: &'a BwxProject,
    pub grid: Vec<GridBar>,
    pub tempos: Vec<TempoEvent>,
    pub tracks: Vec<ExportTrack<'a>>,
}

/// Lays `project` out for a notation format, or says why it cannot be.
pub fn prepare(project: &BwxProject) -> Result<ExportSong<'_>, FormatError> {
    let Some(first) = project.tracks.first() else {
        return Err(FormatError::Incompatible(
            "The project has no tracks.".to_owned(),
        ));
    };
    let horizon = project
        .tracks
        .iter()
        .map(|track| {
            let notes_end = track.notes.iter().map(|note| note.end_tick()).max();
            track.bars_end_tick().max(notes_end.unwrap_or(0))
        })
        .max()
        .unwrap_or(0);
    let report = CompatibilityReport::analyze(project, horizon);
    if !report.standard_exports_enabled {
        return Err(FormatError::Incompatible(report.warning.unwrap_or_else(
            || "The project cannot be flattened to a standard format.".to_owned(),
        )));
    }

    let grid = shared_grid(first, horizon);
    let tempos = tempo_events(project);
    let tracks = project
        .tracks
        .iter()
        .map(|track| ExportTrack {
            track,
            bars: track_bars(track, &grid),
        })
        .collect();
    Ok(ExportSong {
        project,
        grid,
        tempos,
        tracks,
    })
}

fn shared_grid(track: &InstrumentTrack, horizon: Tick) -> Vec<GridBar> {
    let mut grid = Vec::new();
    let mut cursor = 0;
    while grid.len() < track.bar_count.max(1) as usize || cursor < horizon {
        let (start, end) = track.measure_bounds_at(cursor);
        let mark = track.bar_mark_at(start);
        grid.push(GridBar {
            start,
            end,
            signature: track.signature_at(start),
            repeat_open: mark.repeat_open,
            repeat_close: mark.repeat_close,
        });
        cursor = end;
    }
    grid
}

/// The tempo curve every track follows. A progressive point glides when the
/// next point differs.
fn tempo_events(project: &BwxProject) -> Vec<TempoEvent> {
    let own = project
        .per_track_automation
        .then(|| project.tracks.first())
        .flatten()
        .map(|track| &track.tempo_points)
        .filter(|points| !points.is_empty());
    let mut points: Vec<_> = own.unwrap_or(&project.tempo_points).clone();
    points.sort_by_key(|point| point.at_tick);
    let mut events: Vec<TempoEvent> = Vec::new();
    for (index, point) in points.iter().enumerate() {
        let next_differs = points
            .get(index + 1)
            .is_some_and(|next| next.bpm != point.bpm);
        events.push(TempoEvent {
            tick: point.at_tick.max(0),
            bpm: point.bpm,
            linear: point.transition == TempoTransition::Progressive && next_differs,
        });
    }
    if events.is_empty() {
        events.push(TempoEvent {
            tick: 0,
            bpm: 120.0,
            linear: false,
        });
    }
    events
}

impl ExportSong<'_> {
    /// The tempo that holds at the start of bar `bar`, rounded to a whole bpm.
    pub fn bar_tempo(&self, bar: usize) -> u16 {
        let tick = self.grid.get(bar).map_or(0, |bar| bar.start);
        let event = self
            .tempos
            .iter()
            .rev()
            .find(|event| event.tick <= tick)
            .or(self.tempos.first());
        event.map_or(120.0, |event| event.bpm).round().clamp(1.0, 999.0) as u16
    }
}

/// Where inside its bar `tick` sits, as a fraction of the bar.
pub fn bar_fraction(bar: &GridBar, tick: Tick) -> f64 {
    (tick - bar.start) as f64 / (bar.end - bar.start).max(1) as f64
}

/// A note's stretch inside one bar.
#[derive(Clone)]
struct Segment {
    string_index: usize,
    fret: u8,
    velocity: u8,
    effects: NoteEffects,
    /// The bend of the note's whole chain with times counted from the start
    /// of the bar's track (absolute ticks), shifted with the segment.
    bend: Option<PitchEnvelope>,
    start: Tick,
    end: Tick,
    /// The note sounded on before this segment began.
    tied_in: bool,
}

fn track_bars(track: &InstrumentTrack, grid: &[GridBar]) -> Vec<ExportBar> {
    grid.iter().map(|bar| bar_beats(track, bar)).collect()
}

/// A tuplet of the track inside one bar, as a format writes it: its parts
/// keep their exact places instead of snapping to the 64th grid.
struct Group {
    span: TupletSpan,
    /// How far the span's start moved to land on the grid.
    shift: Tick,
    start: Tick,
    end: Tick,
    /// What one part is written as: the span's length over its normal notes.
    unit: Tick,
}

impl Group {
    fn tuplet(&self, starts: bool, ends: bool) -> Tuplet {
        Tuplet {
            enters: self.span.parts,
            times: self.span.normal_notes(),
            unit: self.unit,
            starts,
            ends,
        }
    }

    /// The index of the part boundary nearest `tick`.
    fn boundary_at(&self, tick: Tick) -> u32 {
        (0..=u32::from(self.span.parts))
            .min_by_key(|&index| (self.span.boundary(index) + self.shift - tick).abs())
            .unwrap_or(0)
    }

    /// The tick `written` ticks of written value into the group fall on.
    fn real_at(&self, written: Tick) -> Tick {
        self.start + self.span.real_ticks(written)
    }
}

/// The track's tuplets that fit the bar and that formats can write: every
/// note touching one is its own, and a part is a note value.
fn tuplet_groups(track: &InstrumentTrack, bar: &GridBar, snap: impl Fn(Tick) -> Tick) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    let mut spans = track.tuplets.clone();
    spans.extend(track.loose_tuplets(bar.start, bar.end));
    spans.sort_by_key(|span| span.start);
    for span in &spans {
        if !span.is_tuplet() || span.start < bar.start || span.end() > bar.end {
            continue;
        }
        let normal = Tick::from(span.normal_notes());
        let unit = span.length / normal;
        // Formats count a tuplet in a plain note value (no dots).
        if unit * normal != span.length || !is_note_length(unit) || value_and_dots(unit).1 != 0 {
            continue;
        }
        let own = track
            .notes
            .iter()
            .filter(|note| note.abs_tick < span.end() && note.end_tick() > span.start)
            .all(|note| span.holds(note) || span.continues(note));
        let start = snap(span.start);
        if !own || start < bar.start || start + span.length > bar.end {
            continue;
        }
        if groups.last().is_some_and(|last| start < last.end) {
            continue;
        }
        groups.push(Group {
            span: *span,
            shift: start - span.start,
            start,
            end: start + span.length,
            unit,
        });
    }
    groups
}

fn bar_beats(track: &InstrumentTrack, bar: &GridBar) -> ExportBar {
    let snap = |tick: Tick| bar.start + (tick - bar.start + QUANTUM / 2).div_euclid(QUANTUM) * QUANTUM;
    let groups = tuplet_groups(track, bar, snap);
    // A time that snapped into a tuplet goes to the nearer end of it.
    let keep_out = |tick: Tick| {
        groups
            .iter()
            .find(|group| group.start < tick && tick < group.end)
            .map_or(tick, |group| {
                if tick - group.start < group.end - tick { group.start } else { group.end }
            })
    };

    let mut segments: Vec<Segment> = Vec::new();
    for note in &track.notes {
        if note.abs_tick >= bar.end || note.end_tick() <= bar.start {
            continue;
        }
        let tied_in = note.abs_tick < bar.start || track.is_tied_continuation(note);
        // The bend belongs to the first note of the chain; a later note of
        // it carries the part of the bend that falls on it.
        let head = track.chain_head(note);
        let bend = head.effects.bend.as_ref().map(|envelope| PitchEnvelope {
            points: envelope.points.iter().map(|point| crate::core::pitch::PitchPoint { at: point.at + head.abs_tick, ..*point }).collect(),
        });
        let mut effects = note.effects.clone();
        effects.bend = None;
        let mut push = |start: Tick, end: Tick, tied_in: bool| {
            if start < end {
                segments.push(Segment {
                    string_index: note.string_index,
                    fret: note.fret,
                    velocity: note.velocity,
                    effects: effects.clone(),
                    bend: bend.clone(),
                    start,
                    end,
                    tied_in,
                });
            }
        };
        if let Some(group) = groups.iter().find(|group| group.span.holds(note)) {
            push(note.abs_tick + group.shift, note.end_tick() + group.shift, tied_in);
        } else if let Some(group) = groups.iter().find(|group| group.span.continues(note)) {
            // A note running into or out of a tuplet: the stretch outside is
            // a plain note, the stretch inside a tied one that fills the
            // parts it covers.
            let (entering, leaving) = (note.abs_tick < group.span.start, note.end_tick() > group.span.end());
            if entering {
                push(snap(note.abs_tick.max(bar.start)).min(bar.end - 1), group.start, tied_in);
            }
            let from = if entering { group.start } else { note.abs_tick + group.shift };
            let to = if leaving { group.end } else { note.end_tick() + group.shift };
            push(from, to, tied_in || entering);
            if leaving {
                push(group.end, snap(note.end_tick().min(bar.end)).max(group.end), true);
            }
        } else {
            let start = keep_out(snap(note.abs_tick.max(bar.start)).min(bar.end - 1));
            let end = keep_out(snap(note.end_tick().min(bar.end))).clamp(start + 1, bar.end);
            push(start, end, tied_in);
        }
    }

    // One sounding note per string: a note stops where the next on its
    // string starts.
    segments.sort_by_key(|segment| (segment.string_index, segment.start));
    let mut kept: Vec<Segment> = Vec::new();
    for segment in segments {
        if let Some(last) = kept.last_mut()
            && last.string_index == segment.string_index
        {
            if last.start == segment.start {
                *last = segment;
                continue;
            }
            last.end = last.end.min(segment.start);
        }
        kept.push(segment);
    }

    let mut edges = vec![bar.start, bar.end];
    for segment in &kept {
        edges.push(segment.start);
        edges.push(segment.end);
    }
    for group in &groups {
        edges.push(group.start);
        edges.push(group.end);
    }
    edges.sort_unstable();
    edges.dedup();

    let mut beats = Vec::new();
    for window in edges.windows(2) {
        let (from, to) = (window[0], window[1]);
        let active: Vec<&Segment> = kept
            .iter()
            .filter(|segment| segment.start <= from && segment.end >= to)
            .collect();
        let notes_of = |index: usize, from: Tick, to: Tick| -> Vec<ExportNote> {
            active
                .iter()
                .map(|segment| ExportNote {
                    string_index: segment.string_index,
                    fret: segment.fret,
                    velocity: segment.velocity,
                    effects: NoteEffects {
                        bend: segment
                            .bend
                            .as_ref()
                            .map(|envelope| envelope.window(from, to))
                            .filter(|window| !window.is_flat()),
                        ..segment.effects.clone()
                    },
                    tremolo: None,
                    tied: if index == 0 {
                        segment.start < from || segment.tied_in
                    } else {
                        true
                    },
                })
                .collect()
        };

        if let Some(group) = groups.iter().find(|group| group.start <= from && to <= group.end) {
            let (first, last) = (group.boundary_at(from), group.boundary_at(to));
            if last > first {
                // The stretch is whole parts: cut it into the note values
                // the parts are written in, and lay the pieces out in time.
                let mut written_at = Tick::from(first) * group.unit;
                let lengths = pieces(written_at, Tick::from(last - first) * group.unit);
                let mut at = from;
                for (index, &length) in lengths.iter().enumerate() {
                    written_at += length;
                    let next = if index + 1 == lengths.len() { to } else { group.real_at(written_at) };
                    beats.push(ExportBeat {
                        start: at,
                        duration: (next - at).max(1),
                        written: length,
                        tuplet: Some(group.tuplet(index == 0 && from == group.start, index + 1 == lengths.len() && to == group.end)),
                        notes: notes_of(index, at, next),
                    });
                    at = next;
                }
                continue;
            }
        }

        let mut at = from;
        for (index, length) in pieces(from - bar.start, to - from).into_iter().enumerate() {
            beats.push(ExportBeat {
                start: at,
                duration: length,
                written: length,
                tuplet: None,
                notes: notes_of(index, at, at + length),
            });
            at += length;
        }
    }
    for beat in &mut beats {
        beat.notes.sort_by_key(|note| note.string_index);
        // The tremolo bar moves the whole track, so one note carries it.
        let end = beat.start + beat.duration;
        for dive in track.tremolo_bar.iter().filter(|dive| dive.start < end && dive.end() > beat.start) {
            let window = dive.envelope.window(beat.start - dive.start, end - dive.start);
            if let Some(first) = beat.notes.first_mut()
                && !window.is_flat()
            {
                first.tremolo = Some(window);
            }
        }
    }
    ExportBar {
        empty: kept.is_empty(),
        beats,
    }
}

/// Whether `ticks` is a whole, dotted or double dotted note value.
pub fn is_note_length(ticks: Tick) -> bool {
    VALUES.iter().any(|&value| {
        ticks == value || ticks == value * 3 / 2 || ticks == value * 7 / 4
    })
}

/// Splits a stretch that starts `offset` into its bar into note lengths. A
/// stretch that is one note value stays whole; any other is cut so that each
/// piece starts on a multiple of its own length, the way a bar is written.
fn pieces(offset: Tick, length: Tick) -> Vec<Tick> {
    if is_note_length(length) {
        return vec![length];
    }
    let mut out = Vec::new();
    let mut at = offset;
    let mut left = length;
    while left > 0 {
        let value = VALUES
            .iter()
            .copied()
            .find(|&value| value <= left && at % value == 0)
            .unwrap_or(QUANTUM)
            .min(left);
        out.push(value);
        at += value;
        left -= value;
    }
    out
}

/// A note value's number (1 whole, 2 half ... 64) and dots, for a length
/// that `is_note_length`.
pub fn value_and_dots(ticks: Tick) -> (u32, u8) {
    for (index, &value) in VALUES.iter().enumerate() {
        let number = 1_u32 << index;
        if ticks == value {
            return (number, 0);
        }
        if ticks == value * 3 / 2 {
            return (number, 1);
        }
        if ticks == value * 7 / 4 {
            return (number, 2);
        }
    }
    (64, 0)
}

/// The demo project with its second track's polymeter evened out, so it
/// can be exported.
#[cfg(test)]
pub(crate) fn aligned_demo() -> BwxProject {
    let mut project = BwxProject::demo();
    project.tracks[1].time_signature_changes = project.tracks[0].time_signature_changes.clone();
    project.tracks[1].bar_count = project.tracks[0].bar_count;
    project
}

/// One track with a triplet of eighths, a quintuplet in a half note with a
/// rest part, and a sextuplet whose first note lasts four parts.
#[cfg(test)]
pub(crate) fn tuplet_demo() -> BwxProject {
    let mut project = aligned_demo();
    project.tracks.truncate(1);
    let id = project.tracks[0].id;
    project.tracks[0].notes.clear();
    project.tracks[0].tuplets.clear();
    assert!(!project.write_tuplet(id, 960, 960, 3, 5, 3, 100).is_empty());
    assert!(!project.write_tuplet(id, 3840, 1920, 5, 4, 2, 100).is_empty());
    let span = TupletSpan { start: 7680, length: 1920, parts: 6 };
    assert!(project.tracks[0].add_tuplet(span));
    for (from, parts) in [(0, 4), (4, 1), (5, 1)] {
        let (a, b) = (span.boundary(from), span.boundary(from + parts));
        project.insert_note(id, a, b - a, 3, 5).unwrap();
    }
    project
}

/// A triplet in the last beat of a bar whose first note was emptied, and the
/// note before it stretched into that part: the span stays, and the note is a
/// plain one that continues into the triplet as a tied part.
#[cfg(test)]
pub(crate) fn stretched_tuplet_demo() -> (BwxProject, TupletSpan) {
    let mut project = aligned_demo();
    project.tracks.truncate(1);
    let id = project.tracks[0].id;
    project.tracks[0].notes.clear();
    project.tracks[0].tuplets.clear();
    let span = TupletSpan { start: 6720, length: 960, parts: 3 };
    project.insert_note(id, 5760, 1280, 5, 3).unwrap();
    project.insert_note(id, 7040, 320, 5, 4).unwrap();
    project.insert_note(id, 7360, 320, 5, 5).unwrap();
    assert!(project.tracks[0].add_tuplet(span));
    assert!(!project.tracks[0].prune_tuplets());
    (project, span)
}

/// The notes of a track as (start, length) pairs, for comparing round trips.
#[cfg(test)]
pub(crate) fn note_places(track: &InstrumentTrack) -> Vec<(Tick, Tick)> {
    track.notes.iter().map(|note| (note.abs_tick, note.duration_ticks)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TICKS_PER_QUARTER as Q;

    fn bar(project: &BwxProject, index: usize) -> ExportBar {
        prepare(project).unwrap().tracks[0].bars[index].clone()
    }

    fn lengths(bar: &ExportBar) -> Vec<Tick> {
        bar.beats.iter().map(|beat| beat.duration).collect()
    }

    fn project_with(notes: &[(Tick, Tick, usize, u8)]) -> BwxProject {
        let mut project = BwxProject::demo();
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        for &(start, length, string, fret) in notes {
            project.insert_note(id, start, length, string, fret).unwrap();
        }
        project.tracks.truncate(1);
        project
    }

    #[test]
    fn an_empty_bar_is_a_rest_of_the_bar() {
        let project = project_with(&[]);
        let bar = bar(&project, 0);
        assert!(bar.empty);
        assert_eq!(lengths(&bar), [Q * 4]);
        assert!(bar.beats[0].notes.is_empty());
    }

    #[test]
    fn gaps_become_rests_and_the_beats_tile_the_bar() {
        // Quarter on beat 1, an eighth rest, a quarter, then the bar's end.
        let project = project_with(&[(0, Q, 5, 0), (Q + Q / 2, Q, 4, 2)]);
        let bar = bar(&project, 0);
        assert_eq!(lengths(&bar), [Q, Q / 2, Q, Q + Q / 2]);
        assert_eq!(lengths(&bar).iter().sum::<Tick>(), Q * 4);
        assert!(bar.beats[1].notes.is_empty());
        assert!(bar.beats.iter().skip(1).all(|beat| !beat.notes.iter().any(|note| note.tied)));
    }

    #[test]
    fn a_note_across_the_barline_is_tied_into_the_next_bar() {
        let project = project_with(&[(Q * 3, Q * 2, 5, 3)]);
        let song = prepare(&project).unwrap();
        let first = &song.tracks[0].bars[0];
        let second = &song.tracks[0].bars[1];
        let last = first.beats.last().unwrap();
        assert_eq!((last.start, last.duration), (Q * 3, Q));
        assert!(!last.notes[0].tied);
        assert_eq!(second.beats[0].duration, Q);
        assert!(second.beats[0].notes[0].tied);
    }

    #[test]
    fn a_length_that_is_no_note_value_is_cut_into_tied_pieces() {
        // Five eighths: a half and an eighth, tied.
        let project = project_with(&[(0, Q * 5 / 2, 5, 0)]);
        let bar = bar(&project, 0);
        assert_eq!(&lengths(&bar)[..2], [Q * 2, Q / 2]);
        assert!(!bar.beats[0].notes[0].tied);
        assert!(bar.beats[1].notes[0].tied);
    }

    #[test]
    fn a_dotted_length_stays_one_beat() {
        let project = project_with(&[(0, Q * 3 / 2, 5, 0), (Q * 3 / 2, Q * 7 / 4, 4, 0)]);
        let bar = bar(&project, 0);
        assert_eq!(&lengths(&bar)[..2], [Q * 3 / 2, Q * 7 / 4]);
    }

    #[test]
    fn a_long_note_under_shorter_ones_is_tied_through_their_beats() {
        let project = project_with(&[(0, Q * 2, 5, 0), (Q, Q, 3, 2)]);
        let bar = bar(&project, 0);
        assert_eq!(&lengths(&bar)[..2], [Q, Q]);
        let second = &bar.beats[1];
        assert_eq!(second.notes.len(), 2);
        let low = second.notes.iter().find(|note| note.string_index == 5).unwrap();
        assert!(low.tied);
        let high = second.notes.iter().find(|note| note.string_index == 3).unwrap();
        assert!(!high.tied);
    }

    #[test]
    fn a_note_overlapped_on_its_own_string_ends_where_the_next_starts() {
        let project = project_with(&[(0, Q * 2, 5, 0), (Q, Q, 5, 2)]);
        let bar = bar(&project, 0);
        let on_beat_one = &bar.beats[0];
        assert_eq!(on_beat_one.duration, Q);
        assert!(bar.beats.iter().all(|beat| beat.notes.len() <= 1));
    }

    #[test]
    fn times_snap_to_the_nearest_64th() {
        let project = project_with(&[(7, Q, 5, 0)]);
        let bar = bar(&project, 0);
        assert_eq!(bar.beats[0].start, 0);
        assert!(!bar.beats[0].notes.is_empty());
    }

    #[test]
    fn repeats_and_tempo_follow_the_first_track() {
        let mut project = project_with(&[(0, Q, 5, 0)]);
        let bar_two = project.tracks[0].bar_spans()[1].start;
        project.tracks[0].set_repeat_open(bar_two, true);
        project.tracks[0].set_repeat_close(bar_two, 2);
        project.tempo_points = vec![
            crate::core::TempoPoint::new(0, 100.0),
            crate::core::TempoPoint::new(bar_two, 140.0),
        ];
        let song = prepare(&project).unwrap();
        assert!(song.grid[1].repeat_open);
        assert_eq!(song.grid[1].repeat_close, 2);
        assert_eq!((song.bar_tempo(0), song.bar_tempo(1)), (100, 140));
    }

    #[test]
    fn a_project_with_an_irrational_bar_is_refused() {
        let mut project = project_with(&[(0, Q, 5, 0)]);
        project.tracks[0].time_signature_changes = vec![crate::core::project::TimeSignatureChange {
            at_tick: 0,
            signature: TimeSignature::new(5, 12),
        }];
        assert!(matches!(
            prepare(&project),
            Err(FormatError::Incompatible(_))
        ));
    }

    fn project_with_tuplet(start: Tick, length: Tick, parts: u8, fill: &[(u32, u32)]) -> BwxProject {
        // `fill` lists (first part, parts it lasts) of the notes to write.
        let mut project = project_with(&[]);
        let id = project.tracks[0].id;
        let span = TupletSpan { start, length, parts };
        project.tracks[0].add_tuplet(span);
        for &(first, count) in fill {
            let (from, to) = (span.boundary(first), span.boundary(first + count));
            project.insert_note(id, from, to - from, 5, 0).unwrap();
        }
        project
    }

    #[test]
    fn a_triplet_keeps_its_parts_and_is_written_as_plain_values() {
        let project = project_with_tuplet(0, Q, 3, &[(0, 1), (1, 1), (2, 1)]);
        let bar = bar(&project, 0);
        let triplet: Vec<_> = bar.beats.iter().filter(|beat| beat.tuplet.is_some()).collect();
        assert_eq!(triplet.len(), 3);
        for beat in &triplet {
            assert_eq!((beat.duration, beat.written), (Q / 3, Q / 2));
            assert_eq!(beat.tuplet.map(|tuplet| (tuplet.enters, tuplet.times)), Some((3, 2)));
            assert_eq!(beat.notes.len(), 1);
        }
        assert_eq!(triplet.iter().map(|beat| beat.start).collect::<Vec<_>>(), [0, Q / 3, Q * 2 / 3]);
        assert_eq!(bar.beats.iter().map(|beat| beat.duration).sum::<Tick>(), Q * 4);
    }

    #[test]
    fn empty_parts_of_a_tuplet_are_rests_in_its_values() {
        // One note, then four rest parts of a quintuplet in a half note.
        let project = project_with_tuplet(0, Q * 2, 5, &[(0, 1)]);
        let bar = bar(&project, 0);
        let (note, rest) = (&bar.beats[0], &bar.beats[1]);
        assert_eq!((note.duration, note.written), (Q * 2 / 5, Q / 2));
        assert!(rest.notes.is_empty());
        assert_eq!((rest.duration, rest.written), (Q * 8 / 5, Q * 2));
        assert_eq!(rest.tuplet.map(|tuplet| (tuplet.enters, tuplet.times)), Some((5, 4)));
        assert_eq!(note.duration + rest.duration, Q * 2);
    }

    #[test]
    fn a_stretch_that_is_no_note_value_is_cut_into_tied_pieces_of_the_tuplet() {
        // Five of six parts of a sextuplet: a half and an eighth, tied.
        let project = project_with_tuplet(0, Q * 2, 6, &[(0, 5)]);
        let bar = bar(&project, 0);
        let written: Vec<_> = bar.beats.iter().take(2).map(|beat| beat.written).collect();
        assert_eq!(written, [Q * 2, Q / 2]);
        assert!(bar.beats[1].notes[0].tied);
        assert_eq!(bar.beats[0].duration + bar.beats[1].duration, Q * 5 / 3);
    }

    #[test]
    fn a_power_of_two_split_is_no_tuplet_and_a_foreign_note_cancels_one() {
        let project = project_with_tuplet(0, Q, 4, &[(0, 1), (1, 1), (2, 1), (3, 1)]);
        assert!(bar(&project, 0).beats.iter().all(|beat| beat.tuplet.is_none()));
        // A note that is no part of the span keeps the notes plain.
        let mut project = project_with_tuplet(0, Q, 3, &[(0, 1)]);
        let id = project.tracks[0].id;
        project.insert_note(id, 40, 100, 4, 0).unwrap();
        assert!(bar(&project, 0).beats.iter().all(|beat| beat.tuplet.is_none()));
    }

    #[test]
    fn notes_left_without_a_span_are_still_written_as_the_tuplet_they_form() {
        // The end of the second bar after an edit dropped the span: a note of
        // four parts and two of one part, of a sextuplet in a half note.
        let project = project_with(&[(5760, 1280, 5, 3), (7040, 320, 5, 3), (7360, 320, 5, 3)]);
        assert!(project.tracks[0].tuplets.is_empty());
        let bar = bar(&project, 1);
        let tuplet: Vec<_> = bar.beats.iter().filter(|beat| beat.tuplet.is_some()).collect();
        assert_eq!(tuplet.len(), 3);
        assert!(tuplet.iter().all(|beat| beat.tuplet.map(|tuplet| (tuplet.enters, tuplet.times)) == Some((6, 4))));
        assert_eq!(tuplet.iter().map(|beat| beat.duration).collect::<Vec<_>>(), [1280, 320, 320]);
        assert_eq!(tuplet.iter().map(|beat| beat.written).collect::<Vec<_>>(), [Q * 2, Q / 2, Q / 2]);
        assert_eq!(bar.beats.iter().map(|beat| beat.duration).sum::<Tick>(), Q * 4);
    }

    #[test]
    fn a_note_stretched_into_a_tuplet_is_a_tied_note_filling_its_first_part() {
        // A triplet in the last beat of bar 2 whose first note was replaced
        // by the note before it, stretched into the part.
        let mut project = project_with(&[(5760, 960, 5, 3), (7040, 320, 5, 4), (7360, 320, 5, 4)]);
        let span = TupletSpan { start: 6720, length: 960, parts: 3 };
        assert!(project.tracks[0].add_tuplet(span));
        project.tracks[0].notes[0].duration_ticks = 1280;
        assert!(!project.tracks[0].prune_tuplets());
        let bar = bar(&project, 1);
        let beats: Vec<_> = bar.beats.iter().map(|beat| (beat.start, beat.duration, beat.written, beat.tuplet.is_some(), beat.notes.first().map(|note| note.tied))).collect();
        assert_eq!(
            beats,
            [
                (3840, 1920, 1920, false, None),
                (5760, 960, 960, false, Some(false)),
                (6720, 320, 480, true, Some(true)),
                (7040, 320, 480, true, Some(false)),
                (7360, 320, 480, true, Some(false)),
            ]
        );
    }

    #[test]
    fn an_empty_part_of_a_tuplet_a_note_was_stretched_into_stays_a_rest() {
        let mut project = project_with(&[(5760, 960, 5, 3), (7360, 320, 5, 4)]);
        let span = TupletSpan { start: 6720, length: 960, parts: 3 };
        assert!(project.tracks[0].add_tuplet(span));
        project.tracks[0].notes[0].duration_ticks = 1280;
        assert!(!project.tracks[0].prune_tuplets());
        let bar = bar(&project, 1);
        let last: Vec<_> = bar.beats.iter().skip(3).map(|beat| (beat.start, beat.written, beat.notes.len())).collect();
        assert_eq!(last, [(7040, 480, 0), (7360, 480, 1)]);
    }

    #[test]
    fn the_first_and_last_beat_of_a_tuplet_mark_its_edges() {
        let project = project_with_tuplet(0, Q, 3, &[(0, 1), (1, 1), (2, 1)]);
        let edges: Vec<_> = bar(&project, 0)
            .beats
            .iter()
            .filter_map(|beat| beat.tuplet.map(|tuplet| (tuplet.starts, tuplet.ends, tuplet.unit)))
            .collect();
        assert_eq!(edges, [(true, false, Q / 2), (false, false, Q / 2), (false, true, Q / 2)]);
        // Two in a row are two groups, whatever lies between the notes.
        let mut project = project_with_tuplet(0, Q, 3, &[(0, 1)]);
        let second = TupletSpan { start: Q, length: Q, parts: 3 };
        assert!(project.tracks[0].add_tuplet(second));
        let id = project.tracks[0].id;
        project.insert_note(id, Q, Q / 3, 5, 0).unwrap();
        let bar = bar(&project, 0);
        let starts = bar.beats.iter().filter(|beat| beat.tuplet.is_some_and(|tuplet| tuplet.starts)).count();
        let ends = bar.beats.iter().filter(|beat| beat.tuplet.is_some_and(|tuplet| tuplet.ends)).count();
        assert_eq!((starts, ends), (2, 2));
    }

    #[test]
    fn plain_off_beat_notes_are_not_mistaken_for_a_tuplet() {
        // A note that is merely a little late does not fit any tuplet.
        let project = project_with(&[(0, Q, 5, 0), (Q + 41, Q, 4, 0)]);
        assert!(bar(&project, 0).beats.iter().all(|beat| beat.tuplet.is_none()));
    }

    #[test]
    fn note_lengths_are_told_apart_from_other_lengths() {
        assert!(is_note_length(Q * 3 / 2));
        assert!(is_note_length(Q * 4));
        assert!(!is_note_length(Q * 5 / 2));
        assert_eq!(value_and_dots(Q * 7 / 4), (4, 2));
        assert_eq!(value_and_dots(Q / 4), (16, 0));
    }
}
