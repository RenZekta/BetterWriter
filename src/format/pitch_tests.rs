//! Bends and tremolo bar dives through the file formats.

use crate::core::{
    BwxProject, TICKS_PER_QUARTER as Q,
    pitch::{FULL_BEND, PitchEnvelope},
};

use super::{guitarpro, import_gp, midi, score, tuxguitar};

/// One track: a bend-and-release over a half note, a plain eighth-note pair,
/// and a dive of the tremolo bar over the next quarter.
fn bent_project() -> (BwxProject, u64) {
    let mut project = score::aligned_demo();
    project.tracks.truncate(1);
    let id = project.tracks[0].id;
    project.tracks[0].notes.clear();
    project.tracks[0].tuplets.clear();
    project.tracks[0].annotations.clear();
    project.tracks[0].tremolo_bar.clear();
    let bent = project.insert_note(id, 0, Q * 2, 0, 7).unwrap();
    project.insert_note(id, Q * 2, Q, 1, 5).unwrap();
    let track = project.track_mut(id).unwrap();
    assert!(track.set_bend(bent, Some(PitchEnvelope::bend_release(Q * 2, FULL_BEND))));
    assert!(track.add_dive(Q * 2, &PitchEnvelope::dive(Q, FULL_BEND)).is_some());
    (project, bent)
}

fn assert_bend_and_dive(project: &BwxProject, what: &str) {
    let track = &project.tracks[0];
    let first = track.notes.iter().min_by_key(|note| note.abs_tick).unwrap();
    let bend = first.effects.bend.as_ref().unwrap_or_else(|| panic!("{what}: the bend is lost"));
    assert_eq!(bend.value_at(0), 0.0, "{what}");
    assert_eq!(bend.value_at(Q), 4.0, "{what}: the peak");
    assert_eq!(bend.value_at(Q * 2), 0.0, "{what}: released");
    assert_eq!(track.tremolo_bar.len(), 1, "{what}: the dive is lost");
    let dive = &track.tremolo_bar[0];
    assert_eq!(dive.start, Q * 2, "{what}");
    assert_eq!(dive.envelope.value_at(Q / 2), -4.0, "{what}: the bottom of the dive");
    assert_eq!(dive.envelope.value_at(Q), 0.0, "{what}: back at rest");
    assert!(
        track.notes.iter().filter(|note| note.effects.bend.is_some()).count() == 1,
        "{what}: only the first note carries the bend"
    );
}

#[test]
fn tuxguitar_keeps_a_bend_and_a_dive() {
    let (project, _) = bent_project();
    let bytes = tuxguitar::write_tg(&project).unwrap();
    let import = tuxguitar::import_tg(&bytes).unwrap();
    assert_bend_and_dive(&import.project, "tg");
    assert!(
        !import.report.iter().any(|line| line.contains("bend") || line.contains("tremolo bar")),
        "bends are read now, not reported as skipped: {:?}",
        import.report
    );
}

#[test]
fn guitar_pro_keeps_a_bend_and_a_dive() {
    let (project, _) = bent_project();
    let bytes = guitarpro::write_gp(&project).unwrap();
    let import = import_gp::import_gp(&bytes).unwrap();
    assert_bend_and_dive(&import.project, "gp");
}

#[test]
fn guitar_pro_writes_bends_with_the_property_names_the_format_uses() {
    let (project, _) = bent_project();
    let bytes = guitarpro::write_gp(&project).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("Content/score.gpif").unwrap(), &mut xml).unwrap();
    for name in [
        "Bended",
        "BendOriginValue",
        "BendMiddleValue",
        "BendDestinationOffset",
        "WhammyBar",
        "WhammyBarMiddleValue",
    ] {
        assert!(xml.contains(&format!("name=\"{name}\"")), "{name} missing");
    }
    // A whole tone is 100, a quarter tone 25.
    assert!(xml.contains("<Float>100</Float>"));
    assert!(xml.contains("<Float>-100</Float>"));
}

#[test]
fn a_midi_file_carries_the_bends_as_pitch_wheel_messages() {
    let (project, _) = bent_project();
    let channel = project.tracks[0].channel.min(15);
    let bytes = midi::write_midi(&project).unwrap();
    let wheel = |offset: i16| {
        let value = (8192 + i32::from(offset)) as u16;
        [0xe0 | channel, (value & 0x7f) as u8, (value >> 7) as u8]
    };
    let top = wheel(crate::core::pitch::wheel_offset(4.0));
    let bottom = wheel(crate::core::pitch::wheel_offset(-4.0));
    assert!(bytes.windows(3).any(|window| window == top), "the bend's peak");
    assert!(bytes.windows(3).any(|window| window == bottom), "the dive's bottom");
    // The wheel's range is set to twelve semitones by a registered parameter.
    assert!(bytes.windows(3).any(|window| window == [0xb0 | channel, 6, 12]));
}

#[test]
fn a_project_without_bends_writes_no_pitch_wheel_messages() {
    let project = score::aligned_demo();
    let bytes = midi::write_midi(&project).unwrap();
    let channel = project.tracks[0].channel.min(15);
    assert!(!bytes.windows(1).any(|window| window[0] == 0xe0 | channel));
}

#[test]
fn a_bend_over_a_tied_chain_is_cut_into_the_beats_it_sounds_in() {
    let mut project = score::aligned_demo();
    project.tracks.truncate(1);
    let id = project.tracks[0].id;
    project.tracks[0].notes.clear();
    project.tracks[0].tuplets.clear();
    project.tracks[0].tremolo_bar.clear();
    // One note of two quarters, written as a quarter tied to a quarter in
    // the file formats' beats when something splits it.
    let head = project.insert_note(id, 0, Q, 0, 7).unwrap();
    let tail = project.insert_note(id, Q, Q, 0, 7).unwrap();
    assert!(project.toggle_user_tie(id, tail));
    assert!(project.track_mut(id).unwrap().set_bend(head, Some(PitchEnvelope::bend(Q * 2, FULL_BEND))));

    let bytes = tuxguitar::write_tg(&project).unwrap();
    let import = tuxguitar::import_tg(&bytes).unwrap();
    let track = &import.project.tracks[0];
    let first = track.notes.iter().min_by_key(|note| note.abs_tick).unwrap();
    let bend = first.effects.bend.as_ref().expect("the bend comes back on the first note");
    assert_eq!(bend.value_at(0), 0.0);
    assert_eq!(bend.value_at(Q), 4.0, "the peak sits where the first note ends");
    assert_eq!(bend.value_at(Q * 2), 4.0, "held to the end of the chain");
    assert_eq!(
        track.notes.iter().filter(|note| note.effects.bend.is_some()).count(),
        1,
        "later notes of the chain keep none of their own"
    );
}

#[test]
fn the_native_format_keeps_bends_and_dives_exactly() {
    let (mut project, bent) = bent_project();
    let track = project.track_mut(project.tracks[0].id).unwrap();
    // A curve and an off-grid time survive, which no other format keeps.
    let mut envelope = PitchEnvelope::bend_release(Q * 2, 5);
    envelope.add_point(123, 1);
    envelope.set_curve(2, -40);
    assert!(track.set_bend(bent, Some(envelope.clone())));
    let bytes = super::encode_bwx(&project).unwrap();
    let back = super::decode_bwx(&bytes).unwrap();
    assert_eq!(back, project);
    let note = back.tracks[0].notes.iter().find(|note| note.id == bent).unwrap();
    assert_eq!(note.effects.bend.as_ref(), Some(&envelope));
}

#[test]
fn note_effects_saved_before_bends_existed_still_open() {
    // The native format names its fields, so a note written by an earlier
    // version, with no `bend`, reads with none.
    #[derive(serde::Serialize)]
    struct Earlier {
        dead: bool,
        palm_mute: bool,
        let_ring: bool,
        vibrato: bool,
        slide: bool,
        hammer: bool,
        ghost: bool,
        accent: bool,
        heavy_accent: bool,
        staccato: bool,
        tapping: bool,
        slapping: bool,
        popping: bool,
        fade_in: bool,
    }
    let earlier = Earlier {
        dead: false,
        palm_mute: true,
        let_ring: false,
        vibrato: true,
        slide: false,
        hammer: false,
        ghost: false,
        accent: false,
        heavy_accent: false,
        staccato: false,
        tapping: false,
        slapping: false,
        popping: false,
        fade_in: false,
    };
    let bytes = rmp_serde::to_vec_named(&earlier).unwrap();
    let effects: crate::core::NoteEffects = rmp_serde::from_slice(&bytes).unwrap();
    assert!(effects.palm_mute && effects.vibrato);
    assert_eq!(effects.bend, None);
}
