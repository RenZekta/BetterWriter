//! Guide marks of one track laid over another, for lining up beats and
//! accents between tracks that keep their own bars and tempo.

use std::collections::BTreeMap;

use super::{BwxProject, InstrumentTrack, Tick, bars::{PlayedBar, UnrolledView}};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GhostKind {
    /// A note onset.
    Note,
    /// A note onset carrying an accent.
    Accent,
    /// The start of a bar.
    Bar,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GhostMark {
    /// Position on the target track, between ticks when the tempo maps differ.
    pub tick: f64,
    pub kind: GhostKind,
}

/// The first play of each bar played, as an index into the bars played.
fn first_plays(played: &[PlayedBar]) -> Vec<usize> {
    played
        .iter()
        .map(|bar| {
            played
                .iter()
                .position(|other| other.bar == bar.bar)
                .unwrap_or_default()
        })
        .collect()
}

/// How many bars played of `onto` fold into their first pass while `from`
/// repeats in step with it: the index of the first bar played where the
/// two part ways. A pass folds when the ghost track plays the same pass of
/// the same earlier bars at the same moment.
fn bars_in_step(project: &BwxProject, source: u64, target: u64) -> usize {
    let (Some(from), Some(onto)) = (project.track(source), project.track(target)) else {
        return 0;
    };
    let (from_played, onto_played) = (from.played_bars(), onto.played_bars());
    let (from_first, onto_first) = (first_plays(&from_played), first_plays(&onto_played));
    let (from_map, onto_map) = (project.tempo_map_for(source), project.tempo_map_for(target));
    onto_played
        .iter()
        .enumerate()
        .position(|(index, bar)| {
            from_played.get(index).is_none_or(|other| {
                other.repeated != bar.repeated
                    || other.start != bar.start
                    || other.end != bar.end
                    || from_first[index] != onto_first[index]
                    || (from_map.seconds_at(other.start as f64)
                        - onto_map.seconds_at(bar.start as f64))
                    .abs()
                        >= 1e-6
            })
        })
        .unwrap_or(onto_played.len())
}

/// The track in focus, unrolled where the ghost tracks `sources` shown over
/// it need that, or `None` when it can stay as written. A repeated pass of
/// the focused track folds into its first pass while every ghost track is
/// repeating in step with it. From the first bar played where any of them
/// parts ways, every pass of the focused track is shown, so a loop played 6
/// times against one played 4 times has its first 4 passes folded and the
/// last 2 drawn out. When every repeated pass folds, there is nothing to
/// unroll.
pub fn unrolled_view(project: &BwxProject, sources: &[u64], target: u64) -> Option<UnrolledView> {
    let onto = project.track(target)?;
    if !onto.has_repeats() {
        return None;
    }
    let parting = sources
        .iter()
        .filter(|&&source| source != target && project.track(source).is_some())
        .map(|&source| bars_in_step(project, source, target))
        .min()?;
    let onto_played = onto.played_bars();
    let keep: Vec<bool> = onto_played
        .iter()
        .enumerate()
        .map(|(index, bar)| index >= parting || !bar.repeated)
        .collect();
    let shows_a_repeat = onto_played
        .iter()
        .zip(&keep)
        .any(|(bar, &shown)| bar.repeated && shown);
    if !shows_a_repeat {
        return None;
    }
    UnrolledView::partial(onto, Some(&keep))
}

#[cfg(test)]
pub fn ghost_marks(project: &BwxProject, source: u64, target: u64) -> Vec<GhostMark> {
    ghost_marks_in(
        project,
        &[source],
        target,
        unrolled_view(project, &[source], target).as_ref(),
    )
}

/// The bar starts and note onsets of the ghost tracks `sources`, each as it
/// plays with repeats unrolled, placed where they fall in time on `target`
/// as the canvas draws it: through `unrolled_view`'s layout when there is
/// one, else as written, where a moment inside a repeated region lands on
/// the written position of every pass. Tied continuations are left out, and
/// marks at one position collapse into the strongest. Sorted by position.
pub fn ghost_marks_in(
    project: &BwxProject,
    sources: &[u64],
    target: u64,
    view: Option<&UnrolledView>,
) -> Vec<GhostMark> {
    let mut marks: Vec<GhostMark> = sources
        .iter()
        .filter(|&&source| source != target)
        .flat_map(|&source| source_marks(project, source, target, view))
        .collect();
    marks.sort_by(|a, b| a.tick.total_cmp(&b.tick));
    let mut merged: Vec<GhostMark> = Vec::with_capacity(marks.len());
    for mark in marks {
        match merged.last_mut() {
            Some(last) if (mark.tick - last.tick).abs() < 1e-6 => last.kind = last.kind.max(mark.kind),
            _ => merged.push(mark),
        }
    }
    merged
}

/// The ghost tracks `sources` as tracks of their own whose notes are
/// placed on the timeline of `shown`, the focused track as the canvas draws
/// it (`view` being its unrolled view, if any). Each keeps its instrument
/// and tuning but takes the bars of `shown`, so the editor can paint it
/// with the painters of a real track. Notes sit where they sound in time,
/// through each track's tempo map, and play in order with repeats unrolled;
/// a note played the same on every pass of a folded loop is kept once. The
/// strings are lined up from the lowest one, so a bass lies on the low
/// strings of a guitar.
pub fn ghost_layers(
    project: &BwxProject,
    sources: &[u64],
    shown: &InstrumentTrack,
    view: Option<&UnrolledView>,
) -> Vec<InstrumentTrack> {
    let target = shown.id;
    let target_map = project.tempo_map_for(target);
    let strings = shown.tuning.len();
    let Some(onto) = project.track(target) else {
        return Vec::new();
    };
    sources
        .iter()
        .filter(|&&source| source != target)
        .filter_map(|&source| {
            let from = project.track(source)?;
            let source_map = project.tempo_map_for(source);
            let unrolled = from.unrolled_for_playback();
            let played = unrolled.as_ref().unwrap_or(from);
            let place_at = |tick: Tick, back: f64| -> Tick {
                let playing = target_map.tick_at(source_map.seconds_at(tick as f64)) - back;
                let shown_at = match view {
                    Some(view) => view.shown_tick_at_playback(playing),
                    None => onto.score_tick_at_playback(playing),
                };
                (shown_at + back).round() as Tick
            };
            let place = |tick: Tick| place_at(tick, 0.0);
            // A note's end is looked up just inside the note: an end that is
            // exactly where a repeat pass of the shown track begins would
            // otherwise land on the first bar of the loop.
            let place_end = |tick: Tick| place_at(tick, 1.0);
            // A source string sits `shift` rows lower than its own number.
            let shift = strings as i64 - from.tuning.len() as i64;
            let row = |string: usize| (string as i64 + shift).clamp(0, strings as i64 - 1) as usize;

            let mut layer = from.clone();
            layer.tuning = (0..strings)
                .map(|index| {
                    let at = (index as i64 - shift).clamp(0, from.tuning.len() as i64 - 1);
                    from.tuning[at as usize]
                })
                .collect();
            layer.time_signature_changes = shown.time_signature_changes.clone();
            layer.bar_marks.clear();
            layer.bar_count = shown.bar_count;
            let mut seen = std::collections::HashSet::new();
            layer.notes = played
                .notes
                .iter()
                .filter_map(|note| {
                    let start = place(note.abs_tick);
                    let end = place_end(note.end_tick());
                    let string_index = row(note.string_index);
                    seen.insert((start, string_index, note.fret)).then(|| {
                        let mut ghost = note.clone();
                        ghost.abs_tick = start;
                        ghost.duration_ticks = (end - start).max(1);
                        ghost.string_index = string_index;
                        ghost
                    })
                })
                .collect();
            layer.sort_notes();
            Some(layer)
        })
        .collect()
}

/// The marks of one ghost track on `target`, one per position.
fn source_marks(
    project: &BwxProject,
    source: u64,
    target: u64,
    view: Option<&UnrolledView>,
) -> Vec<GhostMark> {
    let (Some(from), Some(onto)) = (project.track(source), project.track(target)) else {
        return Vec::new();
    };

    let unrolled = from.unrolled_for_playback();
    let played = unrolled.as_ref().unwrap_or(from);
    let mut strongest: BTreeMap<Tick, GhostKind> = BTreeMap::new();
    let mut add = |tick: Tick, kind: GhostKind| {
        let entry = strongest.entry(tick).or_insert(kind);
        *entry = (*entry).max(kind);
    };
    if unrolled.is_some() {
        for bar in from.played_bars() {
            add(bar.start, GhostKind::Bar);
        }
    } else {
        for bar in from.bar_spans() {
            add(bar.start, GhostKind::Bar);
        }
    }
    for note in played.notes.iter().filter(|note| !played.is_tied_continuation(note)) {
        let accented = note.effects.accent || note.effects.heavy_accent;
        add(
            note.abs_tick,
            if accented { GhostKind::Accent } else { GhostKind::Note },
        );
    }

    let source_map = project.tempo_map_for(source);
    let target_map = project.tempo_map_for(target);
    strongest
        .into_iter()
        .map(|(tick, kind)| GhostMark {
            tick: {
                let playing = target_map.tick_at(source_map.seconds_at(tick as f64));
                match view {
                    Some(view) => view.shown_tick_at_playback(playing),
                    None => onto.score_tick_at_playback(playing),
                }
            },
            kind,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        TICKS_PER_QUARTER as Q, TempoTransition,
        automation::{AutomationLane, Scope},
        project::BarMark,
    };

    fn two_tracks() -> (BwxProject, u64, u64) {
        let mut project = BwxProject::demo();
        project.tracks.truncate(2);
        for track in &mut project.tracks {
            track.notes.clear();
            track.time_signature_changes.clear();
            track.bar_marks.clear();
            track.bar_count = 2;
        }
        let (a, b) = (project.tracks[0].id, project.tracks[1].id);
        (project, a, b)
    }

    #[test]
    fn bars_and_notes_of_the_source_land_on_the_same_ticks_at_one_tempo() {
        let (mut project, a, b) = two_tracks();
        project.insert_note(a, Q, Q, 0, 3).unwrap();
        let marks = ghost_marks(&project, a, b);
        let at: Vec<(i64, GhostKind)> = marks
            .iter()
            .map(|mark| (mark.tick.round() as i64, mark.kind))
            .collect();
        assert_eq!(
            at,
            [
                (0, GhostKind::Bar),
                (Q, GhostKind::Note),
                (Q * 4, GhostKind::Bar),
            ]
        );
    }

    #[test]
    fn marks_follow_the_target_tempo() {
        let (mut project, a, b) = two_tracks();
        project.per_track_automation = true;
        project.set_automation_point(AutomationLane::Tempo, Scope::Track(a), 0, 120.0, TempoTransition::Constant);
        project.set_automation_point(AutomationLane::Tempo, Scope::Track(b), 0, 60.0, TempoTransition::Constant);
        let marks = ghost_marks(&project, a, b);
        // A's second bar starts 2 s in; B at half the tempo is two quarters in.
        let second_bar = marks.iter().find(|mark| mark.tick > 1.0).unwrap();
        assert!((second_bar.tick - f64::from(Q as i32) * 2.0).abs() < 1.0);
    }

    #[test]
    fn an_accent_outweighs_a_plain_note_at_the_same_moment() {
        let (mut project, a, b) = two_tracks();
        let first = project.insert_note(a, Q, Q, 0, 3).unwrap();
        project.insert_note(a, Q, Q, 1, 3).unwrap();
        project
            .track_mut(a)
            .unwrap()
            .notes
            .iter_mut()
            .find(|note| note.id == first)
            .unwrap()
            .effects
            .accent = true;
        let marks = ghost_marks(&project, a, b);
        let kinds: Vec<GhostKind> = marks
            .iter()
            .filter(|mark| (mark.tick - Q as f64).abs() < 1.0)
            .map(|mark| mark.kind)
            .collect();
        assert_eq!(kinds, [GhostKind::Accent]);
    }

    #[test]
    fn a_ghost_track_with_a_repeat_shows_every_pass() {
        let (mut project, a, b) = two_tracks();
        project.insert_note(a, Q, Q, 0, 3).unwrap();
        let source = project.track_mut(a).unwrap();
        source.bar_count = 1;
        source.bar_marks = vec![BarMark { at_tick: 0, repeat_open: true, repeat_close: 1 }];
        let marks = ghost_marks(&project, a, b);
        let at: Vec<(i64, GhostKind)> = marks
            .iter()
            .map(|mark| (mark.tick.round() as i64, mark.kind))
            .collect();
        // The bar with its note plays twice, 4 quarters apart.
        assert_eq!(
            at,
            [
                (0, GhostKind::Bar),
                (Q, GhostKind::Note),
                (Q * 4, GhostKind::Bar),
                (Q * 5, GhostKind::Note),
            ]
        );
    }

    fn looped(project: &mut BwxProject, id: u64) {
        let track = project.track_mut(id).unwrap();
        track.bar_count = 1;
        track.bar_marks = vec![BarMark { at_tick: 0, repeat_open: true, repeat_close: 1 }];
    }

    fn unrolled(project: &BwxProject, source: u64, target: u64) -> bool {
        unrolled_view(project, &[source], target).is_some()
    }

    /// Bar 0 of two plays `plays` times, then bar 1.
    fn bar_zero_played(project: &mut BwxProject, id: u64, plays: u16) {
        let track = project.track_mut(id).unwrap();
        track.bar_count = 2;
        track.bar_marks = vec![BarMark { at_tick: 0, repeat_open: true, repeat_close: plays - 1 }];
    }

    #[test]
    fn a_ghost_note_ending_where_a_pass_of_the_shown_loop_begins_keeps_its_length() {
        let (mut project, a, b) = two_tracks();
        // The shown track plays its first bar twice; the ghost track has a
        // note filling that bar and then one filling the next.
        bar_zero_played(&mut project, b, 2);
        project.track_mut(a).unwrap().bar_count = 3;
        project.insert_note(a, 0, Q * 4, 0, 3).unwrap();
        project.insert_note(a, Q * 4, Q * 4, 0, 5).unwrap();
        let shown = project.track(b).unwrap().clone();
        let layers = ghost_layers(&project, &[a], &shown, None);
        let notes = &layers[0].notes;
        assert_eq!(notes.len(), 2);
        // The first note ends where the loop's second pass starts, which is
        // the end of the written bar, not its beginning.
        assert_eq!((notes[0].abs_tick, notes[0].duration_ticks), (0, Q * 4));
    }

    #[test]
    fn extra_passes_of_the_focused_loop_are_drawn_out_and_the_rest_folded() {
        let (mut project, a, b) = two_tracks();
        bar_zero_played(&mut project, a, 4);
        bar_zero_played(&mut project, b, 6);
        let view = unrolled_view(&project, &[a], b).expect("two passes outlast the ghost's loop");
        // Folded passes 1-4 as one bar, passes 5 and 6, then the last bar.
        assert_eq!(view.track.bar_count, 4);
        let bar = (Q * 4) as f64;
        // The first four passes all play on the first bar drawn.
        assert_eq!(view.shown_tick_at_playback(bar * 3.0 + 10.0), 10.0);
        assert_eq!(view.shown_tick_at_playback(bar * 4.0 + 10.0), bar + 10.0);
        assert_eq!(view.shown_tick_at_playback(bar * 5.0 + 10.0), bar * 2.0 + 10.0);
        assert_eq!(view.shown_tick_at_playback(bar * 6.0 + 10.0), bar * 3.0 + 10.0);
        // Only the passes outside the fold read as repeats.
        let flags: Vec<bool> = (0..4).map(|n| view.is_repeated_at(n * (Q * 4) + 1)).collect();
        assert_eq!(flags, [false, true, true, false]);
        // The ghost's last bar starts after its 4th pass, on the 5th of ours.
        let at: Vec<i64> = ghost_marks(&project, a, b).iter().map(|mark| mark.tick.round() as i64).collect();
        assert_eq!(at, [0, Q * 4]);
    }

    #[test]
    fn the_focused_loop_folds_only_as_far_as_every_ghost_track_is_in_step() {
        let (mut project, a, b) = two_tracks();
        let c = project.add_track_like(a).unwrap();
        for track in &mut project.tracks {
            track.notes.clear();
        }
        bar_zero_played(&mut project, a, 4);
        bar_zero_played(&mut project, b, 6);
        bar_zero_played(&mut project, c, 6);
        // A ghost that loops as often as the track in focus changes nothing...
        assert!(unrolled_view(&project, &[c], b).is_none());
        // ... until another ghost track loops less often.
        let view = unrolled_view(&project, &[c, a], b).expect("a outlasted by b");
        assert_eq!(view.track.bar_count, 4);
        assert_eq!(view.track.bar_count, unrolled_view(&project, &[a], b).unwrap().track.bar_count);
    }

    #[test]
    fn marks_of_several_ghost_tracks_merge_by_position() {
        let (mut project, a, b) = two_tracks();
        let c = project.add_track_like(a).unwrap();
        for track in &mut project.tracks {
            track.notes.clear();
            track.bar_count = 2;
        }
        project.insert_note(a, Q, Q, 0, 3).unwrap();
        project.insert_note(c, Q, Q, 0, 3).unwrap();
        project.insert_note(c, Q * 2, Q, 0, 3).unwrap();
        let view = unrolled_view(&project, &[a, c], b);
        let marks = ghost_marks_in(&project, &[a, c], b, view.as_ref());
        let notes: Vec<i64> = marks
            .iter()
            .filter(|mark| mark.kind == GhostKind::Note)
            .map(|mark| mark.tick.round() as i64)
            .collect();
        assert_eq!(notes, [Q, Q * 2]);
    }

    #[test]
    fn a_ghost_layer_puts_the_notes_on_the_focused_timeline_and_lowest_strings() {
        let (mut project, a, b) = two_tracks();
        // A four-string ghost over a six-string track.
        project.track_mut(a).unwrap().tuning = vec![43, 38, 33, 28];
        project.insert_note(a, Q, Q, 3, 5).unwrap();
        let shown = project.track(b).unwrap().clone();
        assert_eq!(shown.tuning.len(), 6);
        let layers = ghost_layers(&project, &[a], &shown, None);
        assert_eq!(layers.len(), 1);
        let layer = &layers[0];
        assert_eq!(layer.notes.len(), 1);
        let note = &layer.notes[0];
        assert_eq!((note.abs_tick, note.duration_ticks), (Q, Q));
        // The lowest string of the ghost is the lowest of the track in focus,
        // and it still sounds the pitch it was written for.
        assert_eq!(note.string_index, 5);
        assert_eq!(layer.tuning.len(), 6);
        assert_eq!(layer.tuning[5], 28);
        assert_eq!(layer.bar_count, shown.bar_count);
    }

    #[test]
    fn a_note_every_folded_pass_plays_is_one_ghost_note() {
        let (mut project, a, b) = two_tracks();
        bar_zero_played(&mut project, a, 3);
        bar_zero_played(&mut project, b, 3);
        project.insert_note(a, 0, Q, 0, 3).unwrap();
        let shown = project.track(b).unwrap().clone();
        assert!(unrolled_view(&project, &[a], b).is_none());
        let layers = ghost_layers(&project, &[a], &shown, None);
        assert_eq!(layers[0].notes.len(), 1);
    }

    #[test]
    fn a_shorter_focused_loop_folds_completely() {
        let (mut project, a, b) = two_tracks();
        bar_zero_played(&mut project, a, 6);
        bar_zero_played(&mut project, b, 4);
        assert!(!unrolled(&project, a, b));
    }

    #[test]
    fn a_focused_track_with_its_own_loop_is_unrolled() {
        let (mut project, a, b) = two_tracks();
        project.insert_note(a, Q * 5, Q, 0, 3).unwrap();
        looped(&mut project, b);
        assert!(unrolled(&project, a, b));
        let marks = ghost_marks(&project, a, b);
        // The source's note falls in the second pass of the target.
        assert!(marks.iter().any(|mark| (mark.tick - (Q * 5) as f64).abs() < 1.0 && mark.kind == GhostKind::Note));
    }

    #[test]
    fn loops_that_play_in_step_need_no_unrolling() {
        let (mut project, a, b) = two_tracks();
        project.insert_note(a, Q, Q, 0, 3).unwrap();
        looped(&mut project, a);
        looped(&mut project, b);
        assert!(!unrolled(&project, a, b));
        // Shown as written: both passes of the ghost land on the one bar.
        let marks = ghost_marks(&project, a, b);
        assert!(marks.iter().all(|mark| mark.tick < (Q * 4) as f64));
    }

    #[test]
    fn loops_of_different_length_or_tempo_are_not_in_step() {
        let (mut project, a, b) = two_tracks();
        looped(&mut project, a);
        looped(&mut project, b);
        project.track_mut(b).unwrap().bar_marks[0].repeat_close = 2;
        assert!(unrolled(&project, a, b));

        let (mut project, a, b) = two_tracks();
        looped(&mut project, a);
        looped(&mut project, b);
        project.per_track_automation = true;
        project.set_automation_point(AutomationLane::Tempo, Scope::Track(a), 0, 120.0, TempoTransition::Constant);
        project.set_automation_point(AutomationLane::Tempo, Scope::Track(b), 0, 60.0, TempoTransition::Constant);
        // Same bars, but the passes do not start together.
        assert!(unrolled(&project, a, b));
    }

    #[test]
    fn a_focused_track_without_loops_is_never_unrolled() {
        let (mut project, a, b) = two_tracks();
        looped(&mut project, a);
        assert!(!unrolled(&project, a, b));
    }

    #[test]
    fn a_track_has_no_ghost_of_itself() {
        let (project, a, _) = two_tracks();
        assert!(ghost_marks(&project, a, a).is_empty());
        assert!(ghost_marks(&project, a, u64::MAX).is_empty());
    }
}
