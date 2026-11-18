use serde::{Deserialize, Serialize};

use super::project::{BwxProject, Tick};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum StandardExportFormat {
    TuxGuitar,
    GuitarPro,
}

impl StandardExportFormat {
    pub fn label(self) -> &'static str {
        match self {
            StandardExportFormat::TuxGuitar => ".tg",
            StandardExportFormat::GuitarPro => ".gp",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompatibilityReport {
    pub has_polymeter: bool,
    pub has_unaligned_bar_boundaries: bool,
    pub has_irrational_meter: bool,
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

        let has_irrational_meter = project.tracks.iter().any(|track| {
            track
                .time_signature_changes
                .iter()
                .any(|change| !change.signature.is_regular())
        });

        let incompatible = has_polymeter || has_unaligned_bar_boundaries || has_irrational_meter;
        let warning = if has_irrational_meter && !(has_polymeter || has_unaligned_bar_boundaries) {
            Some("Irrational time signature, incompatible format.".to_owned())
        } else {
            incompatible.then(|| "Polyrhythms detected, incompatible format.".to_owned())
        };
        Self {
            has_polymeter,
            has_unaligned_bar_boundaries,
            has_irrational_meter,
            standard_exports_enabled: !incompatible,
            warning,
        }
    }
}
