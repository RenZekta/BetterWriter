use std::path::Path;

use thiserror::Error;

use crate::core::BwxProject;

pub mod guitarpro;
pub mod midi;
pub mod musescore;
pub mod score;
pub mod tuxguitar;
pub mod xml;

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
        Some("tg") => export_project(path, project),
        Some(other) => Err(FormatError::UnsupportedFormat(format!(
            ".{other} saving is not implemented yet"
        ))),
        None => save_bwx(path.with_extension("bwx"), project),
    }
}

/// Writes `project` in the standard format its extension names: `.tg`, `.gp`,
/// `.mid` or `.mscz` / `.mscx`.
pub fn export_project(path: impl AsRef<Path>, project: &BwxProject) -> Result<(), FormatError> {
    let path = path.as_ref();
    let bytes = match extension(path).as_deref() {
        Some("tg") => tuxguitar::write_tg(project)?,
        Some("gp") => guitarpro::write_gp(project)?,
        Some("mid" | "midi") => midi::write_midi(project)?,
        Some("mscz") => musescore::write_mscz(project)?,
        Some("mscx") => musescore::write_mscx(project)?,
        Some(other) => {
            return Err(FormatError::UnsupportedFormat(format!(
                ".{other} export is not implemented"
            )));
        }
        None => {
            return Err(FormatError::UnsupportedFormat(
                "file has no extension".to_owned(),
            ));
        }
    };
    std::fs::write(path, bytes)?;
    Ok(())
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
    fn export_picks_the_format_from_the_extension() {
        let dir = std::env::temp_dir().join(format!("bw_export_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let project = score::aligned_demo();
        for (name, magic) in [
            ("a.tg", &b"\x00"[..]),
            ("a.gp", &b"PK"[..]),
            ("a.mid", &b"MThd"[..]),
            ("a.mscz", &b"PK"[..]),
            ("a.mscx", &b"<?xml"[..]),
        ] {
            let path = dir.join(name);
            export_project(&path, &project).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            assert!(bytes.windows(magic.len()).any(|w| w == magic) || name == "a.tg", "{name}");
            assert!(!bytes.is_empty(), "{name}");
        }
        assert!(matches!(
            export_project(dir.join("a.pdf"), &project),
            Err(FormatError::UnsupportedFormat(_))
        ));
        assert!(matches!(
            export_project(dir.join("a.gp"), &BwxProject::demo()),
            Err(FormatError::Incompatible(_))
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

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
