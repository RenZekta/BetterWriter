//! Pitch envelopes: the bend of a note and the dives of the tremolo bar.
//!
//! Both are one shape, a list of points `{time, value, curve}`. Values are in
//! quarter tones (two to a semitone, four to the whole-tone "full bend").
//! Times are ticks from the start of whatever the envelope belongs to: a
//! note's bend runs along the note's whole tied chain, a dive starts where
//! its `TremoloDive` does.

use serde::{Deserialize, Serialize};

use super::project::{InstrumentTrack, TICKS_PER_QUARTER, Tick};

/// An envelope in the shape Guitar Pro stores: see `PitchEnvelope::plateau`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plateau {
    pub origin: i16,
    /// The held pitch and the percentages of the length it starts and ends at.
    pub middle: Option<(i16, i64, i64)>,
    pub destination: i16,
    /// Whether the shape holds the envelope as it is, nothing lost.
    pub exact: bool,
}

/// Largest bend or dive either way, in quarter tones (an octave). The synth's
/// pitch wheel is set to this range, so the whole envelope fits it.
pub const MAX_QUARTER_TONES: i16 = 24;

/// Pitch wheel range the synth is set to, in semitones either way.
pub const WHEEL_RANGE_SEMITONES: u8 = 12;

/// A bend of one whole tone.
pub const FULL_BEND: i16 = 4;

/// Shortest dive of the tremolo bar.
pub const MIN_DIVE_TICKS: Tick = TICKS_PER_QUARTER / 8;

/// Spacing of the pitch wheel messages sent while a segment moves.
const WHEEL_STEP_TICKS: Tick = TICKS_PER_QUARTER / 24;

/// Curve strength at which a segment is as bent as the shape allows.
pub const MAX_CURVE: i8 = 100;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PitchPoint {
    pub at: Tick,
    /// Quarter tones above (positive) or below the written pitch.
    pub value: i16,
    /// Shape of the segment leading into this point, -100 to 100. 0 is a
    /// straight line, a positive curve leaves the previous point slowly and
    /// arrives fast, a negative one the other way round. Ignored on the first
    /// point.
    #[serde(default)]
    pub curve: i8,
}

impl PitchPoint {
    pub fn new(at: Tick, value: i16) -> Self {
        Self { at: at.max(0), value: value.clamp(-MAX_QUARTER_TONES, MAX_QUARTER_TONES), curve: 0 }
    }
}

/// How far along a segment the value has moved when the segment is `t`
/// (0 to 1) of the way through, for a curve strength of `curve`.
fn shape(t: f32, curve: i8) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if curve == 0 {
        return t;
    }
    t.powf(4.0_f32.powf(f32::from(curve) / f32::from(MAX_CURVE)))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PitchEnvelope {
    pub points: Vec<PitchPoint>,
}

impl PitchEnvelope {
    /// Sorts the points by time, lets the later of two points at one time
    /// win, and clamps values and curves into range.
    pub fn new(mut points: Vec<PitchPoint>) -> Self {
        for point in &mut points {
            *point = PitchPoint {
                at: point.at.max(0),
                value: point.value.clamp(-MAX_QUARTER_TONES, MAX_QUARTER_TONES),
                curve: point.curve.clamp(-MAX_CURVE, MAX_CURVE),
            };
        }
        points.sort_by_key(|point| point.at);
        let mut kept: Vec<PitchPoint> = Vec::with_capacity(points.len());
        for point in points {
            match kept.last_mut() {
                Some(last) if last.at == point.at => *last = point,
                _ => kept.push(point),
            }
        }
        Self { points: kept }
    }

    /// Time of the last point.
    pub fn length(&self) -> Tick {
        self.points.last().map_or(0, |point| point.at)
    }

    /// The value at `at`: held before the first point and after the last.
    pub fn value_at(&self, at: Tick) -> f32 {
        let Some(first) = self.points.first() else {
            return 0.0;
        };
        if at <= first.at {
            return f32::from(first.value);
        }
        for pair in self.points.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            if at <= to.at {
                let t = (at - from.at) as f32 / (to.at - from.at).max(1) as f32;
                let moved = shape(t, to.curve);
                return f32::from(from.value) + (f32::from(to.value) - f32::from(from.value)) * moved;
            }
        }
        self.points.last().map_or(0.0, |point| f32::from(point.value))
    }

    /// The furthest the pitch goes from the written one, either way.
    pub fn peak(&self) -> i16 {
        self.points.iter().map(|point| point.value).max_by_key(|value| value.abs()).unwrap_or(0)
    }

    /// Whether the envelope never leaves the written pitch.
    pub fn is_flat(&self) -> bool {
        self.points.iter().all(|point| point.value == 0)
    }

    /// Adds a point, or replaces the one at the same time. Returns its index.
    pub fn add_point(&mut self, at: Tick, value: i16) -> usize {
        let curve = self.points.iter().find(|point| point.at == at).map_or(0, |point| point.curve);
        let mut point = PitchPoint::new(at, value);
        point.curve = curve;
        self.points.push(point);
        *self = Self::new(std::mem::take(&mut self.points));
        self.points.iter().position(|other| other.at == at.max(0)).unwrap_or(0)
    }

    /// Moves a point, keeping it at least `gap` ticks from its neighbours.
    /// Returns its index. The first point may only move in value when
    /// `pin_start` is set.
    pub fn move_point(&mut self, index: usize, at: Tick, value: i16, gap: Tick, pin_start: bool) -> usize {
        let Some(&point) = self.points.get(index) else {
            return index;
        };
        let lower = if index == 0 { 0 } else { self.points[index - 1].at + gap };
        let upper = self.points.get(index + 1).map_or(Tick::MAX, |next| next.at - gap);
        let at = if pin_start && index == 0 { point.at } else { at.clamp(lower, upper.max(lower)) };
        self.points[index] = PitchPoint {
            at,
            value: value.clamp(-MAX_QUARTER_TONES, MAX_QUARTER_TONES),
            curve: point.curve,
        };
        index
    }

    pub fn remove_point(&mut self, index: usize) {
        if index < self.points.len() {
            self.points.remove(index);
        }
    }

    pub fn set_curve(&mut self, index: usize, curve: i8) {
        if let Some(point) = self.points.get_mut(index) {
            point.curve = curve.clamp(-MAX_CURVE, MAX_CURVE);
        }
    }

    /// The envelope cut to `0..=length`: later points go, and a segment
    /// that crossed `length` ends there at the value it had reached.
    pub fn fitted_to(&self, length: Tick) -> Self {
        let length = length.max(0);
        if self.length() <= length {
            return self.clone();
        }
        let mut points: Vec<PitchPoint> = self.points.iter().copied().filter(|point| point.at < length).collect();
        let ends = self.points.iter().find(|point| point.at >= length).map_or(0, |point| point.curve);
        let mut end = PitchPoint::new(length, self.value_at(length).round() as i16);
        end.curve = if points.is_empty() { 0 } else { ends };
        points.push(end);
        Self::new(points)
    }

    /// The part of the envelope in `from..to`, with times counted from
    /// `from`. The edges get points of their own holding the value there.
    pub fn window(&self, from: Tick, to: Tick) -> Self {
        let mut points = vec![PitchPoint::new(0, self.value_at(from).round() as i16)];
        for point in self.points.iter().filter(|point| point.at > from && point.at < to) {
            points.push(PitchPoint { at: point.at - from, ..*point });
        }
        points.push(PitchPoint::new(to - from, self.value_at(to).round() as i16));
        Self::new(points)
    }

    /// The envelope on a grid of `steps + 1` positions across `length`, as a
    /// file format stores it: one value per position the envelope changes at.
    pub fn positions(&self, length: Tick, steps: i32) -> Vec<(i32, i16)> {
        let length = length.max(1);
        let mut out: Vec<(i32, i16)> = Vec::new();
        for position in 0..=steps {
            let tick = length * Tick::from(position) / Tick::from(steps.max(1));
            let value = self.value_at(tick).round() as i16;
            out.push((position, value));
        }
        // A position on a straight line between its neighbours adds nothing.
        let mut kept: Vec<(i32, i16)> = Vec::new();
        for (index, &entry) in out.iter().enumerate() {
            let straight = index > 0
                && index + 1 < out.len()
                && 2 * i32::from(entry.1) == i32::from(out[index - 1].1) + i32::from(out[index + 1].1);
            if !straight {
                kept.push(entry);
            }
        }
        kept
    }

    /// The envelope of a file's points: `(position, value)` pairs over
    /// `steps` positions, spread across `length` ticks. Segments are straight.
    pub fn from_positions(points: &[(i32, i16)], length: Tick, steps: i32) -> Self {
        let steps = Tick::from(steps.max(1));
        let points = points
            .iter()
            .map(|&(position, value)| PitchPoint::new(length * Tick::from(position.max(0)) / steps, value))
            .collect();
        Self::new(points)
    }

    /// Pitch wheel messages that play the envelope between `from` and `to`:
    /// `(tick, offset from the wheel's centre)`. Straight stretches send one
    /// message per point, moving ones one per `WHEEL_STEP_TICKS`.
    pub fn wheel_steps(&self, from: Tick, to: Tick) -> Vec<(Tick, i16)> {
        let mut steps: Vec<(Tick, i16)> = Vec::new();
        let mut push = |tick: Tick, value: f32| {
            let wheel = wheel_offset(value);
            if steps.last().is_none_or(|last| last.1 != wheel) {
                steps.push((tick, wheel));
            }
        };
        push(from, self.value_at(from));
        for pair in self.points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if b.at <= from || a.at >= to {
                continue;
            }
            let lo = a.at.max(from);
            let hi = b.at.min(to);
            if a.value == b.value {
                push(lo, f32::from(a.value));
            } else {
                let mut tick = lo;
                while tick < hi {
                    push(tick, self.value_at(tick));
                    tick += WHEEL_STEP_TICKS;
                }
            }
            push(hi, self.value_at(hi));
        }
        push(to, self.value_at(to));
        steps
    }

    /// The envelope as Guitar Pro keeps one: an origin, a destination and
    /// between them one pitch held from one time to another. `length` is
    /// what the envelope runs across.
    pub fn plateau(&self, length: Tick) -> Plateau {
        let length = length.max(1);
        let percent = |at: Tick| (at * 100 + length / 2) / length;
        let interior: Vec<&PitchPoint> = self.points.iter().filter(|point| point.at > 0 && point.at < length).collect();
        let peak = interior.iter().map(|point| point.value).max_by_key(|value| value.abs());
        let middle = peak.map(|peak| {
            let held: Vec<&&PitchPoint> = interior.iter().filter(|point| point.value == peak).collect();
            let first = held.first().map_or(0, |point| point.at);
            let last = held.last().map_or(0, |point| point.at);
            (peak, percent(first), percent(last))
        });
        let exact = interior.iter().all(|point| Some(point.value) == peak)
            && self.points.iter().skip(1).all(|point| point.curve == 0);
        Plateau {
            origin: self.value_at(0).round() as i16,
            middle,
            destination: self.value_at(length).round() as i16,
            exact,
        }
    }

    /// The envelope of a Guitar Pro bend: origin and destination as
    /// `(percent of the length, value)`, and the held middle pitch as
    /// `(value, percent it starts, percent it ends)`.
    pub fn from_plateau(
        length: Tick,
        origin: (f32, i16),
        middle: Option<(i16, f32, f32)>,
        destination: (f32, i16),
    ) -> Self {
        let at = |percent: f32| (length as f64 * f64::from(percent.clamp(0.0, 100.0)) / 100.0).round() as Tick;
        let mut points = vec![PitchPoint::new(at(origin.0), origin.1)];
        if let Some((value, from, to)) = middle {
            points.push(PitchPoint::new(at(from), value));
            points.push(PitchPoint::new(at(to), value));
        }
        points.push(PitchPoint::new(at(destination.0), destination.1));
        Self::new(points)
    }

    /// A bend of `amount` quarter tones that reaches its pitch halfway
    /// through `length` and holds it.
    pub fn bend(length: Tick, amount: i16) -> Self {
        Self::new(vec![
            PitchPoint::new(0, 0),
            PitchPoint::new(length / 2, amount),
            PitchPoint::new(length, amount),
        ])
    }

    /// A bend that reaches its pitch halfway and comes back to the written one.
    pub fn bend_release(length: Tick, amount: i16) -> Self {
        Self::new(vec![PitchPoint::new(0, 0), PitchPoint::new(length / 2, amount), PitchPoint::new(length, 0)])
    }

    /// A note struck already bent and held there.
    pub fn pre_bend(length: Tick, amount: i16) -> Self {
        Self::new(vec![PitchPoint::new(0, amount), PitchPoint::new(length, amount)])
    }

    /// A note struck already bent that releases to the written pitch halfway.
    pub fn pre_bend_release(length: Tick, amount: i16) -> Self {
        Self::new(vec![PitchPoint::new(0, amount), PitchPoint::new(length / 2, 0), PitchPoint::new(length, 0)])
    }

    /// A dive of the tremolo bar: down by `amount` quarter tones and back up.
    pub fn dive(length: Tick, amount: i16) -> Self {
        Self::new(vec![PitchPoint::new(0, 0), PitchPoint::new(length / 2, -amount.abs()), PitchPoint::new(length, 0)])
    }

    /// A dive as the tremolo bar keeps it: it starts and ends at rest, and is
    /// at least `MIN_DIVE_TICKS` long. `None` when no points are left.
    pub fn as_dive(&self) -> Option<Self> {
        let mut points = self.points.clone();
        let first = points.first()?;
        let start = first.at;
        for point in &mut points {
            point.at -= start;
        }
        points[0] = PitchPoint::new(0, 0);
        if points.len() < 2 {
            points.push(PitchPoint::new(MIN_DIVE_TICKS, 0));
        }
        let end = points.len() - 1;
        points[end].value = 0;
        points[end].at = points[end].at.max(MIN_DIVE_TICKS);
        Some(Self::new(points))
    }
}

/// Offset from the pitch wheel's centre that bends by `quarter_tones`.
pub fn wheel_offset(quarter_tones: f32) -> i16 {
    let per_quarter_tone = 8192.0 / (f32::from(WHEEL_RANGE_SEMITONES) * 2.0);
    (quarter_tones * per_quarter_tone).round().clamp(-8192.0, 8191.0) as i16
}

/// One dive of the tremolo bar. The bar rests at 0 between dives.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TremoloDive {
    pub start: Tick,
    /// Times from `start`. It begins and ends at 0 (`PitchEnvelope::as_dive`).
    pub envelope: PitchEnvelope,
}

impl TremoloDive {
    pub fn end(&self) -> Tick {
        self.start + self.envelope.length()
    }
}

impl InstrumentTrack {
    /// The tick a note's sound ends at: the end of the last note tied to it.
    pub fn sounding_end(&self, note: &super::project::TabNote) -> Tick {
        let mut end = note.end_tick();
        while let Some(next) = self.notes.iter().find(|other| {
            other.string_index == note.string_index
                && other.fret == note.fret
                && other.abs_tick == end
                && other.id != note.id
                && self.is_tied_continuation(other)
        }) {
            end = next.end_tick();
        }
        end
    }

    /// The first note of the chain `note` is tied into (the note itself when
    /// it is not tied to an earlier one).
    pub fn chain_head<'a>(&'a self, note: &'a super::project::TabNote) -> &'a super::project::TabNote {
        let mut head = note;
        while self.is_tied_continuation(head) {
            match self.tie_predecessor(head) {
                Some(before) if before.id != head.id => head = before,
                _ => break,
            }
        }
        head
    }

    /// Gives the note a bend, or takes it off with `None`. The bend runs
    /// along the note's tied chain, so a note tied to the one before it takes
    /// none. Returns whether anything changed.
    pub fn set_bend(&mut self, note_id: u64, bend: Option<PitchEnvelope>) -> bool {
        let Some(index) = self.notes.iter().position(|note| note.id == note_id) else {
            return false;
        };
        if self.is_tied_continuation(&self.notes[index]) {
            return false;
        }
        let length = self.sounding_end(&self.notes[index]) - self.notes[index].abs_tick;
        let bend = bend.map(|envelope| envelope.fitted_to(length)).filter(|envelope| !envelope.points.is_empty());
        if self.notes[index].effects.bend == bend {
            return false;
        }
        self.notes[index].effects.bend = bend;
        true
    }

    /// The dive that covers `tick`, as an index into `tremolo_bar`.
    pub fn dive_at(&self, tick: Tick) -> Option<usize> {
        self.tremolo_bar.iter().position(|dive| dive.start <= tick && tick < dive.end())
    }

    fn dive_fits(&self, skip: Option<usize>, start: Tick, end: Tick) -> bool {
        start >= 0
            && self
                .tremolo_bar
                .iter()
                .enumerate()
                .filter(|(index, _)| Some(*index) != skip)
                .all(|(_, dive)| end <= dive.start || start >= dive.end())
    }

    /// Writes a dive. Fails, returning `None`, where it would overlap another
    /// one. Returns the dive's index.
    pub fn add_dive(&mut self, start: Tick, envelope: &PitchEnvelope) -> Option<usize> {
        let envelope = envelope.as_dive()?;
        if !self.dive_fits(None, start, start + envelope.length()) {
            return None;
        }
        self.tremolo_bar.push(TremoloDive { start, envelope });
        self.tremolo_bar.sort_by_key(|dive| dive.start);
        self.tremolo_bar.iter().position(|dive| dive.start == start)
    }

    /// Replaces the shape of dive `index`. Fails where the new length would
    /// run into the next dive.
    pub fn set_dive(&mut self, index: usize, envelope: &PitchEnvelope) -> bool {
        let (Some(dive), Some(envelope)) = (self.tremolo_bar.get(index), envelope.as_dive()) else {
            return false;
        };
        if !self.dive_fits(Some(index), dive.start, dive.start + envelope.length()) {
            return false;
        }
        self.tremolo_bar[index].envelope = envelope;
        true
    }

    /// Moves dive `index` to begin at `start`. Fails where it would overlap.
    pub fn move_dive(&mut self, index: usize, start: Tick) -> bool {
        let Some(dive) = self.tremolo_bar.get(index) else {
            return false;
        };
        if !self.dive_fits(Some(index), start, start + dive.envelope.length()) {
            return false;
        }
        self.tremolo_bar[index].start = start;
        self.tremolo_bar.sort_by_key(|dive| dive.start);
        true
    }

    pub fn remove_dive(&mut self, index: usize) -> bool {
        if index < self.tremolo_bar.len() {
            self.tremolo_bar.remove(index);
            return true;
        }
        false
    }

    /// Where the tremolo bar is at `tick`, in quarter tones.
    pub fn tremolo_at(&self, tick: Tick) -> f32 {
        self.dive_at(tick)
            .map_or(0.0, |index| self.tremolo_bar[index].envelope.value_at(tick - self.tremolo_bar[index].start))
    }

    /// Moves the dives that start at or after `from` by `delta` ticks, with
    /// the bars.
    pub fn shift_dives(&mut self, from: Tick, delta: Tick) {
        for dive in &mut self.tremolo_bar {
            if dive.start >= from {
                dive.start += delta;
            }
        }
    }

    /// Removes the dives that start in `from..to`.
    pub fn drop_dives_in(&mut self, from: Tick, to: Tick) {
        self.tremolo_bar.retain(|dive| dive.start < from || dive.start >= to);
    }

    /// For a track read from a file that stores a bend on every tied note:
    /// moves the bend of each tied continuation onto the first note of its
    /// chain, so one envelope runs along the whole chain.
    pub fn gather_chain_bends(&mut self) {
        let continuations: Vec<u64> = self
            .notes
            .iter()
            .filter(|note| note.effects.bend.is_some() && self.is_tied_continuation(note))
            .map(|note| note.id)
            .collect();
        for id in continuations {
            let Some(note) = self.notes.iter().find(|note| note.id == id) else {
                continue;
            };
            let (start, envelope) = (note.abs_tick, note.effects.bend.clone().unwrap_or_default());
            let head_id = self.chain_head(note).id;
            let Some(head) = self.notes.iter().find(|note| note.id == head_id) else {
                continue;
            };
            let offset = start - head.abs_tick;
            let mut points: Vec<PitchPoint> = head
                .effects
                .bend
                .iter()
                .flat_map(|bend| bend.points.iter().copied())
                .filter(|point| point.at < offset)
                .collect();
            points.extend(envelope.points.iter().map(|point| PitchPoint { at: point.at + offset, ..*point }));
            if let Some(head) = self.notes.iter_mut().find(|note| note.id == head_id) {
                head.effects.bend = Some(PitchEnvelope::new(points));
            }
            if let Some(note) = self.notes.iter_mut().find(|note| note.id == id) {
                note.effects.bend = None;
            }
        }
    }

    /// Builds the track's dives from the pieces a file stores per note: each
    /// is `(start tick, envelope counted from it)`. Pieces that touch make
    /// one dive.
    pub fn adopt_dives(&mut self, mut pieces: Vec<(Tick, PitchEnvelope)>) {
        pieces.sort_by_key(|piece| piece.0);
        let mut runs: Vec<(Tick, Vec<PitchPoint>)> = Vec::new();
        for (start, envelope) in pieces {
            let absolute = envelope.points.iter().map(|point| PitchPoint { at: point.at + start, ..*point });
            match runs.last_mut() {
                Some((_, points)) if points.last().is_some_and(|last| start <= last.at) => {
                    points.extend(absolute);
                }
                _ => runs.push((start, absolute.collect())),
            }
        }
        for (start, points) in runs {
            let envelope = PitchEnvelope::new(points.into_iter().map(|point| PitchPoint { at: point.at - start, ..point }).collect());
            if !envelope.is_flat() {
                self.add_dive(start, &envelope);
            }
        }
    }

    /// Whether the track has a bend or a dive.
    pub fn has_pitch_envelopes(&self) -> bool {
        !self.tremolo_bar.is_empty() || self.notes.iter().any(|note| note.effects.bend.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::project::BwxProject;

    const Q: Tick = TICKS_PER_QUARTER;

    #[test]
    fn a_straight_segment_is_interpolated_and_the_ends_are_held() {
        let envelope = PitchEnvelope::new(vec![PitchPoint::new(100, 0), PitchPoint::new(300, 4)]);
        assert_eq!(envelope.value_at(0), 0.0);
        assert_eq!(envelope.value_at(200), 2.0);
        assert_eq!(envelope.value_at(900), 4.0);
    }

    #[test]
    fn a_curve_bends_the_segment_without_moving_its_ends() {
        let mut envelope = PitchEnvelope::new(vec![PitchPoint::new(0, 0), PitchPoint::new(400, 4)]);
        let straight = envelope.value_at(200);
        envelope.set_curve(1, 100);
        let late = envelope.value_at(200);
        envelope.set_curve(1, -100);
        let early = envelope.value_at(200);
        assert!(late < straight && straight < early, "{late} {straight} {early}");
        assert_eq!(envelope.value_at(0), 0.0);
        assert_eq!(envelope.value_at(400), 4.0);
    }

    #[test]
    fn points_are_sorted_merged_and_clamped() {
        let envelope = PitchEnvelope::new(vec![
            PitchPoint::new(200, 1),
            PitchPoint::new(0, 0),
            PitchPoint::new(200, 99),
        ]);
        assert_eq!(envelope.points.len(), 2);
        assert_eq!(envelope.points[1].value, MAX_QUARTER_TONES);
    }

    #[test]
    fn moving_a_point_keeps_it_between_its_neighbours() {
        let mut envelope = PitchEnvelope::bend_release(Q * 2, 4);
        let index = envelope.move_point(1, Q * 5, 6, 10, true);
        assert_eq!(envelope.points[index].at, Q * 2 - 10);
        assert_eq!(envelope.points[index].value, 6);
        envelope.move_point(0, 500, 2, 10, true);
        assert_eq!(envelope.points[0].at, 0);
        assert_eq!(envelope.points[0].value, 2);
    }

    #[test]
    fn adding_a_point_at_an_existing_time_replaces_it() {
        let mut envelope = PitchEnvelope::bend(Q, 4);
        let index = envelope.add_point(Q / 2, 1);
        assert_eq!(envelope.points.len(), 3);
        assert_eq!(envelope.points[index].value, 1);
        let before = envelope.add_point(100, 2);
        assert_eq!(before, 1);
        assert_eq!(envelope.points.len(), 4);
    }

    #[test]
    fn presets_have_their_shapes() {
        let length = Q * 2;
        assert_eq!(PitchEnvelope::bend(length, 4).value_at(0), 0.0);
        assert_eq!(PitchEnvelope::bend(length, 4).value_at(length), 4.0);
        assert_eq!(PitchEnvelope::bend_release(length, 4).value_at(length), 0.0);
        assert_eq!(PitchEnvelope::bend_release(length, 4).value_at(length / 2), 4.0);
        assert_eq!(PitchEnvelope::pre_bend(length, 4).value_at(0), 4.0);
        assert_eq!(PitchEnvelope::pre_bend_release(length, 4).value_at(0), 4.0);
        assert_eq!(PitchEnvelope::pre_bend_release(length, 4).value_at(length), 0.0);
    }

    #[test]
    fn fitting_cuts_at_the_length_with_the_value_reached() {
        let envelope = PitchEnvelope::bend(Q * 2, 4);
        let cut = envelope.fitted_to(Q / 2);
        assert_eq!(cut.length(), Q / 2);
        assert_eq!(cut.value_at(Q / 2), envelope.value_at(Q / 2).round());
        assert_eq!(envelope.fitted_to(Q * 4), envelope);
    }

    #[test]
    fn a_window_counts_from_its_start_and_holds_the_edge_values() {
        let envelope = PitchEnvelope::bend_release(Q * 4, 4);
        let window = envelope.window(Q, Q * 3);
        assert_eq!(window.points[0].at, 0);
        assert_eq!(window.points[0].value, 2);
        assert_eq!(window.length(), Q * 2);
        assert_eq!(window.value_at(Q), 4.0);
        assert_eq!(window.value_at(Q * 2), 2.0);
    }

    #[test]
    fn positions_round_trip_through_a_file_grid() {
        let envelope = PitchEnvelope::bend_release(Q * 4, 4);
        let positions = envelope.positions(Q * 4, 12);
        assert_eq!(positions.first(), Some(&(0, 0)));
        assert!(positions.contains(&(6, 4)));
        assert_eq!(positions.last(), Some(&(12, 0)));
        let back = PitchEnvelope::from_positions(&positions, Q * 4, 12);
        assert_eq!(back.value_at(Q * 2), 4.0);
        assert_eq!(back.value_at(Q * 4), 0.0);
        assert_eq!(back.value_at(Q), envelope.value_at(Q));
    }

    #[test]
    fn a_held_stretch_costs_one_position() {
        let positions = PitchEnvelope::bend(Q * 4, 4).positions(Q * 4, 12);
        assert_eq!(positions.first(), Some(&(0, 0)));
        assert!(positions.contains(&(6, 4)));
        assert!(positions.iter().all(|&(position, _)| position <= 6 || position == 12));
    }

    #[test]
    fn the_wheel_follows_the_envelope_and_returns_to_the_start_value() {
        let envelope = PitchEnvelope::bend_release(Q * 2, 4);
        let steps = envelope.wheel_steps(0, Q * 2);
        assert_eq!(steps.first(), Some(&(0, 0)));
        assert_eq!(steps.last().map(|step| step.1), Some(0));
        let top = steps.iter().map(|step| step.1).max().unwrap();
        assert_eq!(top, wheel_offset(4.0));
        assert!(steps.windows(2).all(|pair| pair[0].0 < pair[1].0 && pair[0].1 != pair[1].1));
    }

    #[test]
    fn a_whole_tone_is_a_sixth_of_the_wheel_range() {
        assert_eq!(wheel_offset(0.0), 0);
        assert_eq!(wheel_offset(f32::from(FULL_BEND)), 8192 / 6);
        assert_eq!(wheel_offset(-f32::from(MAX_QUARTER_TONES)), -8192);
    }

    #[test]
    fn a_dive_starts_and_ends_at_rest() {
        let raw = PitchEnvelope::new(vec![PitchPoint::new(100, -4), PitchPoint::new(500, -4)]);
        let dive = raw.as_dive().unwrap();
        assert_eq!(dive.points[0], PitchPoint::new(0, 0));
        assert_eq!(dive.points.last().unwrap().value, 0);
        assert_eq!(dive.length(), 400);
        assert!(PitchEnvelope::default().as_dive().is_none());
        let single = PitchEnvelope::new(vec![PitchPoint::new(0, -2)]).as_dive().unwrap();
        assert_eq!(single.length(), MIN_DIVE_TICKS);
    }

    fn track() -> InstrumentTrack {
        let mut project = BwxProject::demo();
        let mut track = project.tracks.remove(0);
        track.notes.clear();
        track.tremolo_bar.clear();
        track
    }

    #[test]
    fn dives_may_not_overlap_and_stay_in_order() {
        let mut track = track();
        let dive = PitchEnvelope::dive(Q, 4);
        assert_eq!(track.add_dive(Q * 2, &dive), Some(0));
        assert_eq!(track.add_dive(0, &dive), Some(0));
        assert_eq!(track.add_dive(Q * 2 + 10, &dive), None);
        assert_eq!(track.add_dive(Q * 3, &dive), Some(2));
        assert!(!track.move_dive(0, Q * 2 - 10));
        assert!(track.move_dive(0, 100));
        assert!(!track.set_dive(1, &PitchEnvelope::dive(Q * 2, 4)));
        assert!(track.set_dive(1, &PitchEnvelope::dive(Q / 2, 2)));
        assert_eq!(track.tremolo_bar[1].end(), Q * 2 + Q / 2);
    }

    #[test]
    fn the_bar_rests_at_zero_between_dives() {
        let mut track = track();
        track.add_dive(Q, &PitchEnvelope::dive(Q, 4)).unwrap();
        assert_eq!(track.tremolo_at(0), 0.0);
        assert_eq!(track.tremolo_at(Q + Q / 2), -4.0);
        assert_eq!(track.tremolo_at(Q * 3), 0.0);
        assert_eq!(track.dive_at(Q * 2), None);
    }

    #[test]
    fn dives_follow_bar_edits() {
        let mut track = track();
        track.add_dive(Q * 4, &PitchEnvelope::dive(Q, 4)).unwrap();
        track.add_dive(Q * 8, &PitchEnvelope::dive(Q, 4)).unwrap();
        track.shift_dives(Q * 4, Q * 4);
        assert_eq!(track.tremolo_bar.iter().map(|dive| dive.start).collect::<Vec<_>>(), [Q * 8, Q * 12]);
        track.drop_dives_in(Q * 8, Q * 12);
        assert_eq!(track.tremolo_bar.len(), 1);
    }

    fn bar_ticks(track: &InstrumentTrack) -> Tick {
        track.measure_bounds_at(0).1 - track.measure_bounds_at(0).0
    }

    #[test]
    fn dives_move_with_inserted_and_deleted_bars() {
        let mut project = BwxProject::demo();
        let id = project.tracks[0].id;
        let bar = bar_ticks(&project.tracks[0]);
        project.tracks[0].tremolo_bar.clear();
        project.tracks[0].add_dive(bar * 2, &PitchEnvelope::dive(Q, 4)).unwrap();
        assert!(project.insert_bar_before(id, 0));
        assert_eq!(project.tracks[0].tremolo_bar[0].start, bar * 3);
        assert!(project.delete_bar(id, 0));
        assert_eq!(project.tracks[0].tremolo_bar[0].start, bar * 2);
        // Deleting the bar a dive starts in takes the dive with it.
        assert!(project.delete_bar(id, bar * 2));
        assert!(project.tracks[0].tremolo_bar.is_empty());
    }

    #[test]
    fn a_repeated_bar_plays_its_dive_and_its_bend_again() {
        let mut project = BwxProject::demo();
        let id = project.tracks[0].id;
        let bar = bar_ticks(&project.tracks[0]);
        project.tracks[0].notes.clear();
        project.tracks[0].tremolo_bar.clear();
        let note = project.insert_note(id, 0, Q, 0, 5).unwrap();
        let track = project.track_mut(id).unwrap();
        track.set_bend(note, Some(PitchEnvelope::bend(Q, 4)));
        track.add_dive(Q * 2, &PitchEnvelope::dive(Q, 4)).unwrap();
        track.set_repeat_open(0, true);
        track.set_repeat_close(0, 1);
        let played = project.tracks[0].unrolled_for_playback().expect("a repeat unrolls");
        assert_eq!(played.tremolo_bar.iter().map(|dive| dive.start).collect::<Vec<_>>(), [Q * 2, bar + Q * 2]);
        assert_eq!(played.notes.iter().filter(|note| note.effects.bend.is_some()).count(), 2);
    }

    #[test]
    fn playback_events_pair_a_bent_note_with_its_wheel_moves() {
        use crate::core::{MidiEventKind, ShadowTimeline};
        let mut project = BwxProject::demo();
        project.tracks.truncate(1);
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        project.tracks[0].tremolo_bar.clear();
        let plain = project.insert_note(id, 0, Q, 0, 5).unwrap();
        let bent = project.insert_note(id, Q, Q, 1, 5).unwrap();
        project.track_mut(id).unwrap().set_bend(bent, Some(PitchEnvelope::pre_bend(Q, 4)));
        project.track_mut(id).unwrap().add_dive(Q * 3, &PitchEnvelope::dive(Q, 2)).unwrap();
        let events = ShadowTimeline::compile_from_project(&project);
        let chain: Vec<_> = events.iter().filter(|event| event.chain == bent).collect();
        assert_eq!(chain.iter().filter(|event| event.kind == MidiEventKind::NoteOn).count(), 1);
        assert_eq!(chain.iter().filter(|event| event.kind == MidiEventKind::NoteOff).count(), 1);
        let first = chain.iter().min_by_key(|event| (event.tick, event.kind == MidiEventKind::NoteOn)).unwrap();
        assert_eq!(first.kind, MidiEventKind::PitchBend, "a pre-bent note is bent before it is struck");
        assert_eq!(first.bend, crate::core::pitch::wheel_offset(4.0));
        assert!(events.iter().filter(|event| event.chain == plain).count() == 0);
        let dive: Vec<_> = events.iter().filter(|event| event.kind == MidiEventKind::PitchBend && event.chain == 0).collect();
        assert!(dive.len() >= 3);
        assert_eq!(dive.last().map(|event| event.bend), Some(0));
    }

    #[test]
    fn a_bend_survives_tab_view_cutting_its_note_at_a_barline_and_the_merge_back() {
        let mut project = BwxProject::demo();
        let id = project.tracks[0].id;
        let bar = bar_ticks(&project.tracks[0]);
        project.tracks[0].notes.clear();
        let note = project.insert_note(id, bar - Q, Q * 2, 0, 5).unwrap();
        let envelope = PitchEnvelope::bend_release(Q * 2, 4);
        project.track_mut(id).unwrap().set_bend(note, Some(envelope.clone()));

        assert!(project.split_bricks_at_barlines());
        let track = &project.tracks[0];
        assert_eq!(track.notes.len(), 2, "cut into tied notes");
        assert_eq!(track.notes.iter().filter(|note| note.effects.bend.is_some()).count(), 1);
        let head = track.notes.iter().find(|note| note.abs_tick == bar - Q).unwrap();
        assert_eq!(head.effects.bend.as_ref(), Some(&envelope));
        assert_eq!(track.sounding_end(head) - head.abs_tick, Q * 2);

        assert!(project.merge_brick_ties());
        let track = &project.tracks[0];
        assert_eq!(track.notes.len(), 1);
        assert_eq!(track.notes[0].effects.bend.as_ref(), Some(&envelope));
    }

    #[test]
    fn a_bend_runs_along_the_tied_chain_and_cannot_sit_on_a_continuation() {
        let mut project = BwxProject::demo();
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        let head = project.insert_note(id, 0, Q, 0, 5).unwrap();
        let tail = project.insert_note(id, Q, Q, 0, 5).unwrap();
        assert!(project.toggle_user_tie(id, tail));
        let track = project.track_mut(id).unwrap();
        assert!(!track.set_bend(tail, Some(PitchEnvelope::bend(Q, 4))));
        assert!(track.set_bend(head, Some(PitchEnvelope::bend(Q * 4, 4))));
        let bend = track.notes.iter().find(|note| note.id == head).unwrap().effects.bend.clone().unwrap();
        assert_eq!(bend.length(), Q * 2, "cut to the chain");
        assert!(!track.set_bend(head, Some(bend)));
        assert!(track.set_bend(head, None));
        assert!(track.notes.iter().all(|note| note.effects.bend.is_none()));
    }
}
