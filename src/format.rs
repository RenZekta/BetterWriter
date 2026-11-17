use std::{
    fs::File,
    io::{Cursor, Write},
    path::Path,
};

use thiserror::Error;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::core::{BwxProject, CompatibilityReport, StandardExportFormat};

pub mod tuxguitar;

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("MessagePack serialization failed: {0}")]
    MessagePackEncode(#[from] rmp_serde::encode::Error),
    #[error("MessagePack deserialization failed: {0}")]
    MessagePackDecode(#[from] rmp_serde::decode::Error),
    #[error("zip archive failed: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("standard export is disabled: {0}")]
    Incompatible(String),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
}

pub fn encode_bwx(project: &BwxProject) -> Result<Vec<u8>, FormatError> {
    let payload = rmp_serde::to_vec_named(project)?;
    Ok(payload)
}

pub fn decode_bwx(bytes: &[u8]) -> Result<BwxProject, FormatError> {
    let mut project: BwxProject = rmp_serde::from_slice(bytes)?;
    project.ensure_slot_ids();
    Ok(project)
}

pub fn save_bwx(path: impl AsRef<Path>, project: &BwxProject) -> Result<(), FormatError> {
    std::fs::write(path, encode_bwx(project)?)?;
    Ok(())
}

pub fn load_project(path: impl AsRef<Path>) -> Result<BwxProject, FormatError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path)?;
    match extension(path).as_deref() {
        Some("bwx") => decode_bwx(&bytes),
        // `.tg` normalizes its own `bar_count` internally (see
        // `tuxguitar::read_tg`) since it's deriving project state from a
        // foreign format, not round-tripping our own.
        Some("tg") => tuxguitar::read_tg(&bytes),
        Some(other) => Err(FormatError::UnsupportedFormat(format!(
            ".{other} loading is not implemented yet"
        ))),
        None => Err(FormatError::UnsupportedFormat(
            "file has no extension".to_owned(),
        )),
    }
}

pub fn save_project(path: impl AsRef<Path>, project: &BwxProject) -> Result<(), FormatError> {
    let path = path.as_ref();
    match extension(path).as_deref() {
        Some("bwx") => save_bwx(path, project),
        Some("tg") => {
            std::fs::write(path, tuxguitar::write_tg(project)?)?;
            Ok(())
        }
        Some(other) => Err(FormatError::UnsupportedFormat(format!(
            ".{other} saving is not implemented yet"
        ))),
        None => save_bwx(path.with_extension("bwx"), project),
    }
}

pub fn write_multitrack_bundle(
    path: impl AsRef<Path>,
    project: &BwxProject,
    selected_track_ids: &[u64],
    format: StandardExportFormat,
    report: &CompatibilityReport,
) -> Result<(), FormatError> {
    if !report.standard_exports_enabled {
        return Err(FormatError::Incompatible(
            report.warning.clone().unwrap_or_else(|| {
                "The current project cannot be flattened to a standard tab format.".to_owned()
            }),
        ));
    }

    let mut archive = ZipWriter::new(Cursor::new(Vec::<u8>::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for track_id in selected_track_ids {
        let Some(track) = project.track(*track_id) else {
            continue;
        };
        let file_name = format!("{}{}", sanitize_file_name(&track.name), format.label());
        archive.start_file(file_name, options)?;

        // A flattened text listing per track, not a native .tg/.gp file; the
        // compatibility validator decides which projects reach this path.
        let mut text = String::new();
        text.push_str("# BetterWriter compatibility export\n");
        text.push_str(&format!("# Track: {}\n", track.name));
        text.push_str("# Events are absolute ticks in a flattened standard timeline.\n");
        for note in &track.notes {
            text.push_str(&format!(
                "note tick={} duration={} string={} fret={} velocity={}\n",
                note.abs_tick, note.duration_ticks, note.string_index, note.fret, note.velocity
            ));
        }
        archive.write_all(text.as_bytes())?;
    }

    let cursor = archive.finish()?;
    let mut file = File::create(path)?;
    file.write_all(&cursor.into_inner())?;
    Ok(())
}

fn sanitize_file_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();

    if sanitized.is_empty() {
        "track".to_owned()
    } else {
        sanitized
    }
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bwx_round_trip_preserves_tracks() {
        let project = BwxProject::demo();
        let bytes = encode_bwx(&project).unwrap();
        let decoded = decode_bwx(&bytes).unwrap();
        assert_eq!(decoded.tracks.len(), 2);
        assert_eq!(decoded.tracks[0].notes[0].fret, 0);
    }

    #[test]
    fn bwx_round_trip_keeps_repeat_signs_and_the_sound_chain() {
        let mut project = BwxProject::demo();
        let bar = project.tracks[0].bar_spans()[1].start;
        project.tracks[0].set_repeat_open(bar, true);
        project.tracks[0].set_repeat_close(bar, 2);
        let slot = |name: &str, instrument: bool| crate::core::SoundChainSlot {
            id: crate::core::fresh_slot_id(),
            name: name.to_owned(),
            path: format!("/x/{name}.vst3"),
            is_instrument: instrument,
            vendor: "Acme".to_owned(),
            state: vec![1, 2, 3],
            ..Default::default()
        };
        project.tracks[0].sound_chain.push(slot("Amp", false));
        project.tracks[0].sound_chain.push(slot("Piano", true));
        project.tracks[0].sound_chain[0].bypassed = true;
        project.master_chain.push(slot("Verb", false));
        project.master_fx_bypassed = true;
        project.tracks[0].fx_bypassed = true;
        let decoded = decode_bwx(&encode_bwx(&project).unwrap()).unwrap();
        assert_eq!(decoded, project);
        assert!(decoded.tracks[0].fx_bypassed && decoded.master_fx_bypassed);
        assert_eq!(decoded.master_chain.len(), 1);
        assert_ne!(
            decoded.tracks[0].sound_chain[0].id,
            decoded.tracks[0].sound_chain[1].id
        );
        assert_eq!(decoded.tracks[0].bar_mark_at(bar).repeat_close, 2);
    }
}

#[cfg(test)]
mod slot_id_tests {
    use super::*;
    use crate::core::SoundChainSlot;

    #[test]
    fn slots_saved_without_ids_get_distinct_ones_on_load() {
        let mut project = BwxProject::demo();
        for chain in [&mut project.tracks[0].sound_chain, &mut project.master_chain] {
            chain.push(SoundChainSlot::default());
            chain.push(SoundChainSlot::default());
        }
        let decoded = decode_bwx(&encode_bwx(&project).unwrap()).unwrap();
        let mut ids: Vec<u64> = decoded.tracks[0]
            .sound_chain
            .iter()
            .chain(&decoded.master_chain)
            .map(|slot| slot.id)
            .collect();
        assert!(ids.iter().all(|id| *id != 0));
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 4);
    }
}
