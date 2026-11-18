//! Automation curves: tempo, and the volume and pan of a track. Each lane has
//! one curve shared by the whole project, or one per track once the project
//! is set to per-track automation.

use serde::{Deserialize, Serialize};

use super::project::{BwxProject, TempoPoint, TempoTransition, Tick};
use super::sounds::midi_pan;
use super::tempo::TempoMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomationLane {
    Tempo,
    Volume,
    Pan,
}

impl AutomationLane {
    pub const ALL: [AutomationLane; 3] = [
        AutomationLane::Tempo,
        AutomationLane::Volume,
        AutomationLane::Pan,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AutomationLane::Tempo => "Tempo",
            AutomationLane::Volume => "Volume",
            AutomationLane::Pan => "Pan",
        }
    }

    /// The values a point of this lane can hold.
    pub fn limits(self) -> (f32, f32) {
        match self {
            AutomationLane::Tempo => (1.0, 999.0),
            AutomationLane::Volume => (0.0, 127.0),
            AutomationLane::Pan => (-64.0, 63.0),
        }
    }

    /// The values the graph shows, top to bottom.
    pub fn graph_range(self) -> (f32, f32) {
        match self {
            AutomationLane::Tempo => (20.0, 320.0),
            other => other.limits(),
        }
    }
}

/// One point of any lane. A `Progressive` point ramps linearly toward the
/// next point's value; a `Constant` one holds its value until the next point.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct AutomationPoint {
    pub at_tick: Tick,
    pub value: f32,
    pub transition: TempoTransition,
    /// Tempo points only: no "♩=bpm" label above the staff.
    pub hidden: bool,
}

impl From<&TempoPoint> for AutomationPoint {
    fn from(point: &TempoPoint) -> Self {
        Self {
            at_tick: point.at_tick,
            value: point.bpm,
            transition: point.transition,
            hidden: point.hidden,
        }
    }
}

impl From<&AutomationPoint> for TempoPoint {
    fn from(point: &AutomationPoint) -> Self {
        Self {
            at_tick: point.at_tick,
            bpm: point.value,
            transition: point.transition,
            hidden: point.hidden,
        }
    }
}

/// Where a curve lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Shared,
    Track(u64),
}

/// The value a curve holds at `tick`: the latest point at or before it.
/// `None` before the first point.
pub fn value_at(points: &[AutomationPoint], tick: Tick) -> Option<f32> {
    points
        .iter()
        .filter(|point| point.at_tick <= tick)
        .max_by_key(|point| point.at_tick)
        .map(|point| point.value)
}

/// Like `value_at`, but ramps through a `Progressive` point toward the next
/// one. Before the first point it reads the first point's value.
pub fn interpolated_value_at(points: &[AutomationPoint], tick: Tick) -> Option<f32> {
    let mut sorted: Vec<&AutomationPoint> = points.iter().collect();
    sorted.sort_by_key(|point| point.at_tick);
    let Some(index) = sorted.iter().rposition(|point| point.at_tick <= tick) else {
        return sorted.first().map(|point| point.value);
    };
    let governing = sorted[index];
    match sorted.get(index + 1) {
        Some(next) if governing.transition == TempoTransition::Progressive => {
            let span = (next.at_tick - governing.at_tick).max(1) as f32;
            let t = ((tick - governing.at_tick) as f32 / span).clamp(0.0, 1.0);
            Some(governing.value + (next.value - governing.value) * t)
        }
        _ => Some(governing.value),
    }
}

/// The controller changes a volume or pan curve asks for, as (tick,
/// controller value) in time order. A constant point sets its value at its
/// tick; a progressive one also steps through every controller value between
/// its own and the next point's.
pub fn controller_steps(lane: AutomationLane, points: &[AutomationPoint]) -> Vec<(Tick, u8)> {
    let controller = |value: f32| -> u8 {
        let rounded = value.round();
        match lane {
            AutomationLane::Pan => midi_pan(rounded.clamp(-64.0, 63.0) as i8),
            _ => rounded.clamp(0.0, 127.0) as u8,
        }
    };
    let mut sorted: Vec<&AutomationPoint> = points.iter().collect();
    sorted.sort_by_key(|point| point.at_tick);
    let mut steps: Vec<(Tick, u8)> = Vec::new();
    let push = |tick: Tick, value: u8, steps: &mut Vec<(Tick, u8)>| {
        if steps.last().is_none_or(|&(_, last)| last != value) {
            steps.push((tick, value));
        }
    };
    for (index, point) in sorted.iter().enumerate() {
        push(point.at_tick, controller(point.value), &mut steps);
        let Some(next) = sorted.get(index + 1) else {
            continue;
        };
        if point.transition != TempoTransition::Progressive || next.value == point.value {
            continue;
        }
        let (from, to) = (controller(point.value), controller(next.value));
        let span = (next.at_tick - point.at_tick) as f64;
        let (low, high) = (from.min(to), from.max(to));
        let crossings: Vec<u8> = if from < to {
            (low + 1..high).collect()
        } else {
            (low + 1..high).rev().collect()
        };
        let offset = match lane {
            AutomationLane::Pan => 64.0,
            _ => 0.0,
        };
        for value in crossings {
            let progress =
                (f64::from(value) - offset - f64::from(point.value)) / f64::from(next.value - point.value);
            let tick = point.at_tick + (progress * span).round() as Tick;
            push(tick.clamp(point.at_tick, next.at_tick), value, &mut steps);
        }
    }
    steps
}

impl BwxProject {
    /// Where the curves that govern `track_id` live.
    pub fn scope_for(&self, track_id: u64) -> Scope {
        if self.per_track_automation {
            Scope::Track(track_id)
        } else {
            Scope::Shared
        }
    }

    fn tempo_curve(&self, scope: Scope) -> &[TempoPoint] {
        match scope {
            Scope::Track(id) => self
                .track(id)
                .map(|track| track.tempo_points.as_slice())
                .filter(|points| !points.is_empty())
                .unwrap_or(&self.tempo_points),
            Scope::Shared => &self.tempo_points,
        }
    }

    fn level_curve(&self, lane: AutomationLane, scope: Scope) -> &[AutomationPoint] {
        let shared = match lane {
            AutomationLane::Volume => &self.volume_points,
            _ => &self.pan_points,
        };
        match scope {
            Scope::Shared => shared,
            Scope::Track(id) => self.track(id).map_or(&[][..], |track| match lane {
                AutomationLane::Volume => track.volume_points.as_slice(),
                _ => track.pan_points.as_slice(),
            }),
        }
    }

    pub fn automation_points(&self, lane: AutomationLane, scope: Scope) -> Vec<AutomationPoint> {
        match lane {
            AutomationLane::Tempo => self.tempo_curve(scope).iter().map(Into::into).collect(),
            _ => self.level_curve(lane, scope).to_vec(),
        }
    }

    fn store_points(&mut self, lane: AutomationLane, scope: Scope, points: Vec<AutomationPoint>) {
        match (lane, scope) {
            (AutomationLane::Tempo, Scope::Shared) => {
                self.tempo_points = points.iter().map(Into::into).collect();
            }
            (AutomationLane::Tempo, Scope::Track(id)) => {
                if let Some(track) = self.track_mut(id) {
                    track.tempo_points = points.iter().map(Into::into).collect();
                }
            }
            (AutomationLane::Volume, Scope::Shared) => self.volume_points = points,
            (_, Scope::Shared) => self.pan_points = points,
            (AutomationLane::Volume, Scope::Track(id)) => {
                if let Some(track) = self.track_mut(id) {
                    track.volume_points = points;
                }
            }
            (_, Scope::Track(id)) => {
                if let Some(track) = self.track_mut(id) {
                    track.pan_points = points;
                }
            }
        }
    }

    /// Inserts or replaces the point at `at_tick`, clamping `value` to what
    /// the lane allows. `false` when nothing changed.
    pub fn set_automation_point(
        &mut self,
        lane: AutomationLane,
        scope: Scope,
        at_tick: Tick,
        value: f32,
        transition: TempoTransition,
    ) -> bool {
        if !value.is_finite() {
            return false;
        }
        let (low, high) = lane.limits();
        let value = value.clamp(low, high);
        let at_tick = at_tick.max(0);
        let mut points = self.automation_points(lane, scope);
        if let Some(point) = points.iter_mut().find(|point| point.at_tick == at_tick) {
            if point.value == value && point.transition == transition {
                return false;
            }
            point.value = value;
            point.transition = transition;
        } else {
            points.push(AutomationPoint {
                at_tick,
                value,
                transition,
                hidden: false,
            });
            points.sort_by_key(|point| point.at_tick);
        }
        self.store_points(lane, scope, points);
        true
    }

    pub fn set_automation_point_hidden(
        &mut self,
        lane: AutomationLane,
        scope: Scope,
        at_tick: Tick,
        hidden: bool,
    ) -> bool {
        let mut points = self.automation_points(lane, scope);
        let Some(point) = points.iter_mut().find(|point| point.at_tick == at_tick) else {
            return false;
        };
        if point.hidden == hidden {
            return false;
        }
        point.hidden = hidden;
        self.store_points(lane, scope, points);
        true
    }

    /// Removes the point at `at_tick`. Tempo keeps its first point: playback
    /// always needs a starting tempo.
    pub fn delete_automation_point(
        &mut self,
        lane: AutomationLane,
        scope: Scope,
        at_tick: Tick,
    ) -> bool {
        if lane == AutomationLane::Tempo && at_tick <= 0 {
            return false;
        }
        let mut points = self.automation_points(lane, scope);
        let before = points.len();
        points.retain(|point| point.at_tick != at_tick);
        if points.len() == before {
            return false;
        }
        self.store_points(lane, scope, points);
        true
    }

    /// Drops every point of the lane. Tempo keeps one flat point at tick 0
    /// holding the tempo the curve started with.
    pub fn clear_automation(&mut self, lane: AutomationLane, scope: Scope) {
        let points = match lane {
            AutomationLane::Tempo => {
                let start = self
                    .automation_points(lane, scope)
                    .first()
                    .map_or(120.0, |point| point.value);
                vec![AutomationPoint::from(&TempoPoint::new(0, start))]
            }
            _ => Vec::new(),
        };
        self.store_points(lane, scope, points);
    }

    /// Switches between one set of curves for the whole project and one set
    /// per track. Turning it on gives every track without a curve of its own
    /// a copy of the shared one, so nothing sounds different until a curve is
    /// edited. Curves of a track are kept while the switch is off.
    pub fn set_per_track_automation(&mut self, on: bool) -> bool {
        if self.per_track_automation == on {
            return false;
        }
        self.per_track_automation = on;
        if on {
            let (tempo, volume, pan) = (
                self.tempo_points.clone(),
                self.volume_points.clone(),
                self.pan_points.clone(),
            );
            for track in &mut self.tracks {
                if track.tempo_points.is_empty() {
                    track.tempo_points = tempo.clone();
                }
                if track.volume_points.is_empty() {
                    track.volume_points = volume.clone();
                }
                if track.pan_points.is_empty() {
                    track.pan_points = pan.clone();
                }
            }
        }
        true
    }

    /// The tempo map that times `track_id`.
    pub fn tempo_map_for(&self, track_id: u64) -> TempoMap {
        TempoMap::new(self.tempo_curve(self.scope_for(track_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::project::TICKS_PER_QUARTER;

    const Q: Tick = TICKS_PER_QUARTER;

    fn point(at_tick: Tick, value: f32, transition: TempoTransition) -> AutomationPoint {
        AutomationPoint {
            at_tick,
            value,
            transition,
            hidden: false,
        }
    }

    #[test]
    fn a_flat_curve_holds_the_latest_point() {
        let points = [
            point(0, 80.0, TempoTransition::Constant),
            point(Q, 100.0, TempoTransition::Constant),
        ];
        assert_eq!(value_at(&points, Q - 1), Some(80.0));
        assert_eq!(value_at(&points, Q), Some(100.0));
        assert_eq!(value_at(&[], 0), None);
        assert_eq!(value_at(&points[1..], 0), None);
    }

    #[test]
    fn a_ramp_reaches_the_next_value_at_its_tick() {
        let points = [
            point(0, 0.0, TempoTransition::Progressive),
            point(Q * 4, 100.0, TempoTransition::Constant),
        ];
        assert_eq!(interpolated_value_at(&points, Q * 2), Some(50.0));
        assert_eq!(interpolated_value_at(&points, Q * 4), Some(100.0));
        assert_eq!(interpolated_value_at(&points[1..], 0), Some(100.0));
    }

    #[test]
    fn a_constant_point_is_one_controller_change() {
        let points = [
            point(0, 100.0, TempoTransition::Constant),
            point(Q, 40.0, TempoTransition::Constant),
        ];
        assert_eq!(
            controller_steps(AutomationLane::Volume, &points),
            [(0, 100), (Q, 40)]
        );
    }

    #[test]
    fn a_ramp_steps_through_every_controller_value() {
        let points = [
            point(0, 10.0, TempoTransition::Progressive),
            point(Q * 4, 14.0, TempoTransition::Constant),
        ];
        let steps = controller_steps(AutomationLane::Volume, &points);
        let values: Vec<u8> = steps.iter().map(|&(_, value)| value).collect();
        assert_eq!(values, [10, 11, 12, 13, 14]);
        assert_eq!(steps[0].0, 0);
        assert_eq!(steps[2].0, Q * 2);
        assert_eq!(steps[4].0, Q * 4);
    }

    #[test]
    fn a_falling_ramp_steps_down_and_pan_is_offset_from_centre() {
        let points = [
            point(0, 2.0, TempoTransition::Progressive),
            point(Q * 2, -2.0, TempoTransition::Constant),
        ];
        let values: Vec<u8> = controller_steps(AutomationLane::Pan, &points)
            .iter()
            .map(|&(_, value)| value)
            .collect();
        assert_eq!(values, [66, 65, 64, 63, 62]);
    }

    #[test]
    fn repeated_values_make_no_extra_changes() {
        let points = [
            point(0, 90.0, TempoTransition::Constant),
            point(Q, 90.2, TempoTransition::Constant),
        ];
        assert_eq!(controller_steps(AutomationLane::Volume, &points), [(0, 90)]);
    }

    #[test]
    fn values_are_clamped_to_the_lane() {
        let mut project = BwxProject::demo();
        let shared = Scope::Shared;
        assert!(project.set_automation_point(AutomationLane::Volume, shared, 0, 500.0, TempoTransition::Constant));
        assert_eq!(project.volume_points[0].value, 127.0);
        assert!(project.set_automation_point(AutomationLane::Pan, shared, 0, -500.0, TempoTransition::Constant));
        assert_eq!(project.pan_points[0].value, -64.0);
        assert!(!project.set_automation_point(AutomationLane::Pan, shared, 0, f32::NAN, TempoTransition::Constant));
    }

    #[test]
    fn tempo_keeps_its_first_point_but_levels_can_be_emptied() {
        let mut project = BwxProject::demo();
        let shared = Scope::Shared;
        assert!(!project.delete_automation_point(AutomationLane::Tempo, shared, 0));
        project.set_automation_point(AutomationLane::Volume, shared, 0, 80.0, TempoTransition::Constant);
        assert!(project.delete_automation_point(AutomationLane::Volume, shared, 0));
        assert!(project.volume_points.is_empty());

        project.set_automation_point(AutomationLane::Tempo, shared, Q, 90.0, TempoTransition::Constant);
        project.clear_automation(AutomationLane::Tempo, shared);
        assert_eq!(project.tempo_points.len(), 1);
        assert_eq!(project.tempo_points[0].bpm, 120.0);
    }

    #[test]
    fn per_track_curves_are_independent_and_start_as_copies() {
        let mut project = BwxProject::demo();
        let (first, second) = (project.tracks[0].id, project.tracks[1].id);
        project.set_automation_point(AutomationLane::Tempo, Scope::Shared, Q * 4, 90.0, TempoTransition::Constant);
        assert!(project.set_per_track_automation(true));
        assert!(!project.set_per_track_automation(true));
        assert_eq!(project.tracks[0].tempo_points, project.tempo_points);

        let scope = project.scope_for(first);
        assert_eq!(scope, Scope::Track(first));
        project.set_automation_point(AutomationLane::Tempo, scope, 0, 200.0, TempoTransition::Constant);
        let first_map = project.tempo_map_for(first);
        let second_map = project.tempo_map_for(second);
        assert!((first_map.seconds_at(Q as f64) - 0.3).abs() < 1e-9);
        assert!((second_map.seconds_at(Q as f64) - 0.5).abs() < 1e-9);
        assert_eq!(project.tempo_points[0].bpm, 120.0);
    }

    #[test]
    fn turning_per_track_off_reads_the_shared_curve_again_and_keeps_the_rest() {
        let mut project = BwxProject::demo();
        let first = project.tracks[0].id;
        project.set_per_track_automation(true);
        project.set_automation_point(AutomationLane::Tempo, Scope::Track(first), 0, 200.0, TempoTransition::Constant);
        project.set_per_track_automation(false);
        let map = project.tempo_map_for(first);
        assert!((map.seconds_at(Q as f64) - 0.5).abs() < 1e-9);
        assert_eq!(project.tracks[0].tempo_points[0].bpm, 200.0);
    }

    #[test]
    fn an_empty_per_track_tempo_curve_falls_back_to_the_shared_one() {
        let mut project = BwxProject::demo();
        project.per_track_automation = true;
        let id = project.tracks[0].id;
        assert!(project.tracks[0].tempo_points.is_empty());
        let map = project.tempo_map_for(id);
        assert!((map.seconds_at(Q as f64) - 0.5).abs() < 1e-9);
    }
}
