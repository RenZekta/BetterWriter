//! A project laid out the way notation formats store it: one shared bar
//! grid, and for each track the beats of every bar. The beats tile each bar
//! without gaps or overlaps, every length is a (dotted) note value, and
//! anything longer is tied across beats or barlines.

use crate::core::{
    BwxProject, CompatibilityReport, NoteEffects, TICKS_PER_QUARTER, Tick,
    project::{InstrumentTrack, TempoTransition, TimeSignature},
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
    pub effects: NoteEffects,
    /// Sounds on from the previous beat's note of the same string.
    pub tied: bool,
}

/// One beat of a track; no notes makes it a rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportBeat {
    pub start: Tick,
    pub duration: Tick,
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
    start: Tick,
    end: Tick,
    /// The note sounded on before this segment began.
    tied_in: bool,
}

fn track_bars(track: &InstrumentTrack, grid: &[GridBar]) -> Vec<ExportBar> {
    grid.iter().map(|bar| bar_beats(track, bar)).collect()
}

fn bar_beats(track: &InstrumentTrack, bar: &GridBar) -> ExportBar {
    let snap = |tick: Tick| bar.start + (tick - bar.start + QUANTUM / 2).div_euclid(QUANTUM) * QUANTUM;

    let mut segments: Vec<Segment> = Vec::new();
    for note in &track.notes {
        if note.abs_tick >= bar.end || note.end_tick() <= bar.start {
            continue;
        }
        let start = snap(note.abs_tick.max(bar.start)).min(bar.end - 1);
        let end = snap(note.end_tick().min(bar.end)).clamp(start + 1, bar.end);
        segments.push(Segment {
            string_index: note.string_index,
            fret: note.fret,
            velocity: note.velocity,
            effects: note.effects.clone(),
            start,
            end,
            tied_in: note.abs_tick < bar.start || track.is_tied_continuation(note),
        });
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
    edges.sort_unstable();
    edges.dedup();

    let mut beats = Vec::new();
    for window in edges.windows(2) {
        let (from, to) = (window[0], window[1]);
        let active: Vec<&Segment> = kept
            .iter()
            .filter(|segment| segment.start <= from && segment.end >= to)
            .collect();
        let mut at = from;
        for (index, length) in pieces(from - bar.start, to - from).into_iter().enumerate() {
            let notes = active
                .iter()
                .map(|segment| ExportNote {
                    string_index: segment.string_index,
                    fret: segment.fret,
                    velocity: segment.velocity,
                    effects: segment.effects.clone(),
                    tied: if index == 0 {
                        segment.start < from || segment.tied_in
                    } else {
                        true
                    },
                })
                .collect();
            beats.push(ExportBeat {
                start: at,
                duration: length,
                notes,
            });
            at += length;
        }
    }
    for beat in &mut beats {
        beat.notes.sort_by_key(|note| note.string_index);
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

    #[test]
    fn note_lengths_are_told_apart_from_other_lengths() {
        assert!(is_note_length(Q * 3 / 2));
        assert!(is_note_length(Q * 4));
        assert!(!is_note_length(Q * 5 / 2));
        assert_eq!(value_and_dots(Q * 7 / 4), (4, 2));
        assert_eq!(value_and_dots(Q / 4), (16, 0));
    }
}
