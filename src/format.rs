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
    Ok(rmp_serde::from_slice(bytes)?)
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

        // This placeholder keeps the archive contract wired while native .tg/.gp
        // writers are implemented behind the compatibility validator.
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
}
