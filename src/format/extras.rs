//! BetterWriter's own settings carried inside a foreign file that has no
//! place for them (the automatic sound, the instrument, the plug-in
//! chains), so that saving and reopening loses none of them. The settings
//! travel as text in a custom property of the score, which the program that
//! owns the format keeps untouched.

use serde::{Deserialize, Serialize};

use crate::core::{
    BwxProject, Tick,
    instruments,
    pitch::{PitchEnvelope, TremoloDive},
    project::SoundChainSlot,
    sounds::variant_for_instrument,
};

use super::FormatError;

#[derive(Serialize, Deserialize, Default)]
struct TrackExtras {
    instrument_id: String,
    sound_auto: bool,
    sound_chain: Vec<SoundChainSlot>,
    fx_bypassed: bool,
    /// Bends, by the start and string of the note that carries them: the
    /// file has no bend that holds a smooth curve.
    #[serde(default)]
    bends: Vec<(Tick, usize, PitchEnvelope)>,
    /// The tremolo bar's dives.
    #[serde(default)]
    dives: Vec<TremoloDive>,
}

#[derive(Serialize, Deserialize, Default)]
struct Extras {
    tracks: Vec<TrackExtras>,
    master_chain: Vec<SoundChainSlot>,
    master_fx_bypassed: bool,
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let word = chunk.iter().enumerate().fold(0_u32, |word, (at, byte)| word | u32::from(*byte) << (16 - 8 * at));
        for at in 0..4 {
            if at <= chunk.len() {
                out.push(ALPHABET[(word >> (18 - 6 * at) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut word = 0_u32;
    let mut bits = 0;
    for ch in text.bytes().filter(|ch| !ch.is_ascii_whitespace() && *ch != b'=') {
        let value = ALPHABET.iter().position(|&c| c == ch)? as u32;
        word = word << 6 | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((word >> bits & 0xff) as u8);
        }
    }
    Some(out)
}

/// The name of the score property that holds the settings.
pub(super) const PROPERTY: &str = "BetterWriter";

/// The settings of `project` as text for a score property.
pub(super) fn encode(project: &BwxProject) -> Result<String, FormatError> {
    let extras = Extras {
        tracks: project
            .tracks
            .iter()
            .map(|track| TrackExtras {
                instrument_id: track.instrument_id.clone(),
                sound_auto: track.sound_auto,
                sound_chain: track.sound_chain.clone(),
                fx_bypassed: track.fx_bypassed,
                bends: track
                    .notes
                    .iter()
                    .filter_map(|note| Some((note.abs_tick, note.string_index, note.effects.bend.clone()?)))
                    .collect(),
                dives: track.tremolo_bar.clone(),
            })
            .collect(),
        master_chain: project.master_chain.clone(),
        master_fx_bypassed: project.master_fx_bypassed,
    };
    Ok(base64_encode(&rmp_serde::to_vec_named(&extras)?))
}

/// Puts settings written by `encode` back onto the tracks of `project`, in
/// order. Text that is not ours, or damaged, changes nothing.
pub(super) fn apply(project: &mut BwxProject, text: &str) {
    let Some(extras) = base64_decode(text).and_then(|bytes| rmp_serde::from_slice::<Extras>(&bytes).ok()) else {
        return;
    };
    for (track, saved) in project.tracks.iter_mut().zip(&extras.tracks) {
        if let Some(def) = instruments::instrument(&saved.instrument_id) {
            track.set_instrument(def);
            track.stringed_variant = variant_for_instrument(def);
        }
        track.sound_auto = saved.sound_auto;
        if saved.sound_auto {
            track.set_sound_automatic();
        }
        track.sound_chain = saved.sound_chain.clone();
        track.fx_bypassed = saved.fx_bypassed;
        for (tick, string, envelope) in &saved.bends {
            let note = track.notes.iter().find(|note| note.abs_tick == *tick && note.string_index == *string);
            if let Some(id) = note.map(|note| note.id) {
                track.set_bend(id, Some(envelope.clone()));
            }
        }
        for dive in &saved.dives {
            track.add_dive(dive.start, &dive.envelope);
        }
    }
    project.master_chain = extras.master_chain;
    project.master_fx_bypassed = extras.master_fx_bypassed;
    project.ensure_slot_ids();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::score;

    #[test]
    fn base64_round_trips_every_length() {
        for length in 0..40_u8 {
            let bytes: Vec<u8> = (0..length).map(|i| i.wrapping_mul(37)).collect();
            assert_eq!(base64_decode(&base64_encode(&bytes)).unwrap(), bytes);
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
    }

    #[test]
    fn bends_and_dives_come_back_onto_the_tracks() {
        use crate::core::{TICKS_PER_QUARTER as Q, pitch::PitchEnvelope};
        let mut project = score::aligned_demo();
        let id = project.tracks[0].id;
        project.tracks[0].notes.clear();
        project.tracks[0].tremolo_bar.clear();
        let note = project.insert_note(id, Q, Q * 2, 1, 5).unwrap();
        let track = project.track_mut(id).unwrap();
        track.set_bend(note, Some(PitchEnvelope::bend_release(Q * 2, 4)));
        track.add_dive(Q * 4, &PitchEnvelope::dive(Q, 6)).unwrap();
        let text = encode(&project).unwrap();

        let mut other = score::aligned_demo();
        other.tracks[0].notes.clear();
        other.tracks[0].tremolo_bar.clear();
        let id = other.tracks[0].id;
        other.insert_note(id, Q, Q * 2, 1, 5).unwrap();
        apply(&mut other, &text);
        assert_eq!(other.tracks[0].notes[0].effects.bend, project.tracks[0].notes[0].effects.bend);
        assert_eq!(other.tracks[0].tremolo_bar, project.tracks[0].tremolo_bar);
    }

    #[test]
    fn settings_come_back_onto_the_tracks() {
        let mut project = score::aligned_demo();
        project.tracks[0].set_instrument(instruments::instrument("electric-guitar-jazz").unwrap_or_else(|| {
            instruments::INSTRUMENTS.iter().find(|def| def.id.starts_with("electric-guitar")).unwrap()
        }));
        project.tracks[0].set_sound_automatic();
        project.tracks[1].set_sound(0, 30);
        project.tracks[1].fx_bypassed = true;
        let text = encode(&project).unwrap();
        let mut bare = score::aligned_demo();
        bare.tracks[0].sound_auto = false;
        bare.tracks[0].instrument_id = String::new();
        bare.tracks[1].sound_auto = true;
        apply(&mut bare, &text);
        assert_eq!(bare.tracks[0].instrument_id, project.tracks[0].instrument_id);
        assert!(bare.tracks[0].sound_auto);
        assert!(!bare.tracks[1].sound_auto);
        assert!(bare.tracks[1].fx_bypassed);
    }

    #[test]
    fn text_that_is_not_ours_changes_nothing() {
        let mut project = score::aligned_demo();
        let before = project.clone();
        apply(&mut project, "not base64 at all !!!");
        apply(&mut project, "AAAA");
        assert!(project == before);
    }
}
