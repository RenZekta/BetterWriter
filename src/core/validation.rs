use serde::{Deserialize, Serialize};

use super::project::{BwxProject, Tick};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompatibilityReport {
    pub has_polymeter: bool,
    pub has_unaligned_bar_boundaries: bool,
    pub has_irrational_meter: bool,
    /// Tracks disagree on repeat signs or, with per-track automation, on the
    /// tempo; standard formats keep both once for the whole score.
    pub has_diverging_tracks: bool,
    pub standard_exports_enabled: bool,
    pub warning: Option<String>,
}

impl CompatibilityReport {
    pub fn analyze(project: &BwxProject, horizon_tick: Tick) -> Self {
        let mut boundaries_by_track = Vec::new();

        for track in &project.tracks {
            let mut cursor = 0;
            let mut boundaries = Vec::new();
            while cursor <= horizon_tick.max(1) {
                boundaries.push(cursor);
                let signature = track.signature_at(cursor);
                cursor += signature.measure_ticks().max(1);
            }
            boundaries_by_track.push(boundaries);
        }

        let has_unaligned_bar_boundaries = boundaries_by_track
            .windows(2)
            .any(|pair| pair[0] != pair[1]);

        let first_signature = project
            .tracks
            .first()
            .and_then(|track| track.time_signature_changes.first())
            .map(|change| change.signature);
        let has_polymeter = project.tracks.iter().any(|track| {
            track
                .time_signature_changes
                .first()
                .map(|change| change.signature)
                != first_signature
        });

        // Denominators past a 64th have no note value in the target formats.
        let has_irrational_meter = project.tracks.iter().any(|track| {
            track
                .time_signature_changes
                .iter()
                .any(|change| !change.signature.is_regular() || change.signature.denominator > 64)
        });

        let first = project.tracks.first();
        let has_diverging_tracks = project.tracks.iter().skip(1).any(|track| {
            let first = first.expect("a second track implies a first");
            track.bar_marks != first.bar_marks
                || (project.per_track_automation && track.tempo_points != first.tempo_points)
        });

        let incompatible = has_polymeter
            || has_unaligned_bar_boundaries
            || has_irrational_meter
            || has_diverging_tracks;
        let warning = if has_polymeter || has_unaligned_bar_boundaries {
            Some("Polyrhythms detected, incompatible format.".to_owned())
        } else if has_irrational_meter {
            Some("Irrational time signature, incompatible format.".to_owned())
        } else if has_diverging_tracks {
            Some("Tracks differ in repeats or tempo, incompatible format.".to_owned())
        } else {
            None
        };
        Self {
            has_polymeter,
            has_unaligned_bar_boundaries,
            has_irrational_meter,
            has_diverging_tracks,
            standard_exports_enabled: !incompatible,
            warning,
        }
    }
}
