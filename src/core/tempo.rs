//! Converting between ticks and real time through a tempo curve.

use super::project::{TICKS_PER_QUARTER, TempoPoint, TempoTransition, Tick};

pub fn seconds_per_tick(bpm: f64) -> f64 {
    60.0 / (bpm.max(1.0) * TICKS_PER_QUARTER as f64)
}

/// One stretch of constant tempo.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Segment {
    start_tick: Tick,
    start_seconds: f64,
    seconds_per_tick: f64,
    /// The rate in thousandths of a bpm, for exact arithmetic.
    rate_milli_bpm: u64,
    /// A ramp is played at its average rate, so its length in time is an
    /// approximation.
    ramp: bool,
}

/// A tempo curve laid out as segments of constant rate, anchored to elapsed
/// seconds. A `Progressive` point plays at the average of its own and the
/// next point's bpm.
#[derive(Clone, Debug, PartialEq)]
pub struct TempoMap {
    segments: Vec<Segment>,
}

/// A time as an exact fraction of a second (numerator, positive denominator).
pub type Seconds = (i128, i128);

impl TempoMap {
    pub fn new(points: &[TempoPoint]) -> Self {
        let mut sorted: Vec<&TempoPoint> = points.iter().collect();
        sorted.sort_by_key(|point| point.at_tick);
        let mut segments: Vec<Segment> = Vec::with_capacity(sorted.len() + 1);
        let mut elapsed = 0.0_f64;
        for (index, point) in sorted.iter().enumerate() {
            let next = sorted.get(index + 1);
            let ramp = point.transition == TempoTransition::Progressive
                && next.is_some_and(|next| next.bpm != point.bpm);
            let rate = match next {
                Some(next) if point.transition == TempoTransition::Progressive => {
                    (f64::from(point.bpm) + f64::from(next.bpm)) / 2.0
                }
                _ => f64::from(point.bpm),
            };
            if index == 0 && point.at_tick > 0 {
                segments.push(Segment {
                    start_tick: 0,
                    start_seconds: 0.0,
                    seconds_per_tick: seconds_per_tick(rate),
                    rate_milli_bpm: milli(rate),
                    ramp,
                });
                elapsed = point.at_tick as f64 * seconds_per_tick(rate);
            }
            let per_tick = seconds_per_tick(rate);
            segments.push(Segment {
                start_tick: point.at_tick.max(0),
                start_seconds: elapsed,
                seconds_per_tick: per_tick,
                rate_milli_bpm: milli(rate),
                ramp,
            });
            if let Some(next) = next {
                elapsed += (next.at_tick - point.at_tick).max(0) as f64 * per_tick;
            }
        }
        if segments.is_empty() {
            segments.push(Segment {
                start_tick: 0,
                start_seconds: 0.0,
                seconds_per_tick: seconds_per_tick(120.0),
                rate_milli_bpm: 120_000,
                ramp: false,
            });
        }
        Self { segments }
    }

    fn segment_at_tick(&self, tick: Tick) -> &Segment {
        let index = self
            .segments
            .iter()
            .rposition(|segment| segment.start_tick <= tick)
            .unwrap_or(0);
        &self.segments[index]
    }

    /// Elapsed seconds at `tick`.
    pub fn seconds_at(&self, tick: f64) -> f64 {
        let segment = self.segment_at_tick(tick.floor() as Tick);
        segment.start_seconds + (tick - segment.start_tick as f64).max(0.0) * segment.seconds_per_tick
    }

    /// The tick playing at `seconds`.
    pub fn tick_at(&self, seconds: f64) -> f64 {
        let index = self
            .segments
            .iter()
            .rposition(|segment| segment.start_seconds <= seconds)
            .unwrap_or(0);
        let segment = &self.segments[index];
        segment.start_tick as f64
            + (seconds - segment.start_seconds).max(0.0) / segment.seconds_per_tick
    }

    /// The exact length of `start..end` as a fraction of a second, or `None`
    /// when a ramp lies inside it (ramps are only approximated) or the
    /// numbers outgrow the arithmetic.
    pub fn exact_seconds(&self, start: Tick, end: Tick) -> Option<Seconds> {
        let mut total: Seconds = (0, 1);
        let mut cursor = start;
        while cursor < end {
            let index = self
                .segments
                .iter()
                .rposition(|segment| segment.start_tick <= cursor)
                .unwrap_or(0);
            let segment = &self.segments[index];
            let stop = self
                .segments
                .get(index + 1)
                .map_or(end, |next| next.start_tick.min(end));
            if segment.ramp {
                return None;
            }
            // ticks * 60 / (bpm * ticks per quarter), with bpm in thousandths
            let part: Seconds = (
                i128::from(stop - cursor) * 60_000,
                i128::from(segment.rate_milli_bpm) * i128::from(TICKS_PER_QUARTER),
            );
            total = add_fractions(total, part)?;
            cursor = stop;
        }
        Some(total)
    }
}

fn milli(bpm: f64) -> u64 {
    (bpm * 1000.0).round().max(1.0) as u64
}

pub fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

pub fn reduce((numerator, denominator): Seconds) -> Seconds {
    let divisor = gcd(numerator, denominator).max(1);
    (numerator / divisor, denominator / divisor)
}

pub fn add_fractions(a: Seconds, b: Seconds) -> Option<Seconds> {
    let denominator = a.1.checked_mul(b.1)?;
    let numerator = a.0.checked_mul(b.1)?.checked_add(b.0.checked_mul(a.1)?)?;
    Some(reduce((numerator, denominator)))
}

/// Bars of one track from a chosen bar onward, timed exactly.
#[derive(Clone, Debug, PartialEq)]
pub struct BarRun {
    /// When the first bar starts.
    pub start: Seconds,
    /// The length of each written bar from there. The last one is taken to
    /// go on repeating after the written bars run out.
    pub lengths: Vec<Seconds>,
}

impl BarRun {
    fn length(&self, index: usize) -> Option<Seconds> {
        self.lengths.get(index).or(self.lengths.last()).copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resync {
    /// The two runs share a bar line after this many bars of each.
    After { first: usize, second: usize },
    /// Their bar lines never meet.
    Never,
    /// A tempo ramp or very large numbers keep it from being worked out.
    Unknown,
}

/// How far two runs of bars have to play before bar lines of both fall on
/// the same moment. Runs that start together are not in sync yet by that
/// alone: the answer is their next shared bar line.
pub fn resync(first: &BarRun, second: &BarRun) -> Resync {
    // Bar lines that have not met within this many bars of one run are
    // taken never to meet.
    const LIMIT: usize = 1_000;
    let (mut a, mut b) = (first.start, second.start);
    let (mut bars_a, mut bars_b) = (0, 0);
    for _ in 0..LIMIT {
        let (Some(left), Some(right)) = (a.0.checked_mul(b.1), b.0.checked_mul(a.1)) else {
            return Resync::Unknown;
        };
        match left.cmp(&right) {
            std::cmp::Ordering::Equal if bars_a + bars_b > 0 => {
                return Resync::After {
                    first: bars_a,
                    second: bars_b,
                };
            }
            std::cmp::Ordering::Less | std::cmp::Ordering::Equal => {
                let Some(next) = first.length(bars_a).and_then(|length| add_fractions(a, length))
                else {
                    return Resync::Unknown;
                };
                a = next;
                bars_a += 1;
            }
            std::cmp::Ordering::Greater => {
                let Some(next) = second.length(bars_b).and_then(|length| add_fractions(b, length))
                else {
                    return Resync::Unknown;
                };
                b = next;
                bars_b += 1;
            }
        }
    }
    Resync::Never
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q: Tick = TICKS_PER_QUARTER;

    fn flat(bpm: f32) -> TempoMap {
        TempoMap::new(&[TempoPoint::new(0, bpm)])
    }

    #[test]
    fn a_quarter_lasts_sixty_over_bpm_seconds() {
        let map = flat(120.0);
        assert!((map.seconds_at(Q as f64) - 0.5).abs() < 1e-9);
        assert!((map.tick_at(0.5) - Q as f64).abs() < 1e-6);
    }

    #[test]
    fn a_later_point_changes_the_rate_from_its_tick() {
        let map = TempoMap::new(&[TempoPoint::new(0, 120.0), TempoPoint::new(Q * 4, 60.0)]);
        assert!((map.seconds_at((Q * 4) as f64) - 2.0).abs() < 1e-9);
        assert!((map.seconds_at((Q * 5) as f64) - 3.0).abs() < 1e-9);
        assert!((map.tick_at(3.0) - (Q * 5) as f64).abs() < 1e-6);
    }

    #[test]
    fn a_ramp_plays_at_its_average_rate() {
        let mut start = TempoPoint::new(0, 100.0);
        start.transition = TempoTransition::Progressive;
        let map = TempoMap::new(&[start, TempoPoint::new(Q * 4, 140.0)]);
        // 120 bpm on average over four quarters
        assert!((map.seconds_at((Q * 4) as f64) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_curve_starting_after_zero_still_covers_the_start() {
        let map = TempoMap::new(&[TempoPoint::new(Q, 120.0)]);
        assert!((map.seconds_at(0.0)).abs() < 1e-9);
        assert!((map.seconds_at(Q as f64) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn exact_lengths_are_fractions_and_ramps_are_refused() {
        let map = flat(90.0);
        // four quarters at 90 bpm last 8/3 s
        assert_eq!(map.exact_seconds(0, Q * 4), Some((8, 3)));

        let mut ramp = TempoPoint::new(0, 100.0);
        ramp.transition = TempoTransition::Progressive;
        let ramped = TempoMap::new(&[ramp, TempoPoint::new(Q * 4, 140.0)]);
        assert_eq!(ramped.exact_seconds(0, Q * 2), None);
        assert!(ramped.exact_seconds(Q * 4, Q * 6).is_some());
    }

    #[test]
    fn exact_lengths_add_up_across_a_tempo_change() {
        let map = TempoMap::new(&[TempoPoint::new(0, 120.0), TempoPoint::new(Q * 2, 60.0)]);
        // two quarters at 120 (1 s) and two at 60 (2 s)
        assert_eq!(map.exact_seconds(0, Q * 4), Some((3, 1)));
    }

    fn run(start: Seconds, length: Seconds) -> BarRun {
        BarRun {
            start,
            lengths: vec![length],
        }
    }

    #[test]
    fn bar_lines_of_a_four_four_and_a_five_eight_track_meet_again() {
        // 2 s bars against 1.25 s bars, the second starting a quarter second
        // late: 4 s is the first moment both have a bar line.
        let result = resync(&run((0, 1), (2, 1)), &run((1, 4), (5, 4)));
        assert_eq!(result, Resync::After { first: 2, second: 3 });
    }

    #[test]
    fn an_offset_the_bar_lengths_cannot_close_never_resyncs() {
        let result = resync(&run((0, 1), (2, 1)), &run((1, 10), (5, 4)));
        assert_eq!(result, Resync::Never);
    }

    #[test]
    fn equal_bars_with_an_offset_never_meet_and_runs_starting_together_meet_later() {
        assert_eq!(resync(&run((0, 1), (2, 1)), &run((1, 1), (2, 1))), Resync::Never);
        assert_eq!(
            resync(&run((3, 2), (2, 1)), &run((3, 2), (5, 4))),
            Resync::After { first: 5, second: 8 }
        );
    }

    #[test]
    fn written_bars_count_before_the_last_one_repeats() {
        let first = BarRun {
            start: (0, 1),
            lengths: vec![(1, 1), (3, 1)],
        };
        // Bar lines at 0, 1, 4, 7, ... never land on the half seconds of the
        // second run, but do land on the whole ones of the third.
        let second = run((1, 2), (2, 1));
        assert_eq!(resync(&first, &second), Resync::Never);
        let meeting = run((1, 1), (3, 1));
        assert_eq!(resync(&first, &meeting), Resync::After { first: 1, second: 0 });
    }

    #[test]
    fn an_empty_run_cannot_be_worked_out() {
        let none = BarRun {
            start: (0, 1),
            lengths: Vec::new(),
        };
        assert_eq!(resync(&none, &run((1, 2), (1, 1))), Resync::Unknown);
    }
}
