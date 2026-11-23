//! The track table's Instrument picker and Sound menu, laid out like
//! MuseScore's: instruments filed by genre and family, sounds in the menu it
//! shows for `MS Basic` with a "Choose automatically" entry on top.

use super::*;
use crate::core::instruments::{self, COMMON_GENRE, GENRES, GROUPS, InstrumentDef};
use crate::core::sounds::{self, MENU, SoundMenu};

/// The open Instrument picker.
pub(super) struct InstrumentPicker {
    track_id: u64,
    filter: InstrumentFilter,
}

impl InstrumentPicker {
    pub(super) fn new(track_id: u64) -> Self {
        Self {
            track_id,
            filter: InstrumentFilter::default(),
        }
    }
}

/// What an instrument list shows: the genre and family chosen and the
/// search text.
pub(super) struct InstrumentFilter {
    /// `None` lists every instrument.
    genre: Option<usize>,
    /// `None` lists every family of the genre.
    group: Option<usize>,
    search: String,
}

impl Default for InstrumentFilter {
    fn default() -> Self {
        Self {
            genre: Some(COMMON_GENRE),
            group: None,
            search: String::new(),
        }
    }
}

/// The instrument list shared by the Instrument picker and the New Project
/// window: search and genre on top, families on the left, instruments on the
/// right. Returns the instrument clicked, if any. `current` is marked.
pub(super) fn instrument_list(
    ui: &mut egui::Ui,
    filter: &mut InstrumentFilter,
    current: &str,
) -> Option<&'static InstrumentDef> {
    let mut chosen: Option<&'static InstrumentDef> = None;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut filter.search)
                .hint_text("Search instruments")
                .desired_width(200.0),
        );
        egui::ComboBox::from_id_salt("instrument_genre")
            .selected_text(genre_label(filter.genre))
            .show_ui(ui, |ui| {
                let mut options = vec![Some(COMMON_GENRE), None];
                options.extend((1..GENRES.len()).map(Some));
                for genre in options {
                    if ui
                        .selectable_label(filter.genre == genre, genre_label(genre))
                        .clicked()
                    {
                        filter.genre = genre;
                        filter.group = None;
                    }
                }
            });
    });
    ui.separator();
    let searching = !filter.search.trim().is_empty();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(170.0, ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.add_enabled_ui(!searching, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("instrument_groups")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if ui
                                .selectable_label(filter.group.is_none(), "All families")
                                .clicked()
                            {
                                filter.group = None;
                            }
                            for group in instruments::groups_of(filter.genre) {
                                if ui
                                    .selectable_label(
                                        filter.group == Some(group),
                                        GROUPS[group].name,
                                    )
                                    .clicked()
                                {
                                    filter.group = Some(group);
                                }
                            }
                        });
                });
            },
        );
        ui.separator();
        // The row is horizontal; the list itself runs downwards.
        ui.vertical(|ui| {
            egui::ScrollArea::vertical()
                .id_salt("instrument_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let group = if searching { None } else { filter.group };
                    let found = instruments::listed(filter.genre, group, &filter.search);
                    if found.is_empty() {
                        ui.weak("No instrument matches.");
                    }
                    for def in found {
                        let response = ui
                            .add(
                                egui::Button::selectable(def.id == current, def.title())
                                    .min_size(Vec2::new(ui.available_width(), 0.0)),
                            )
                            .on_hover_text(GROUPS[usize::from(def.group)].name);
                        if response.clicked() {
                            chosen = Some(def);
                        }
                    }
                });
        });
    });
    chosen
}

fn genre_label(genre: Option<usize>) -> &'static str {
    genre.map_or("All instruments", |index| GENRES[index].name)
}

impl BetterWriterApp {
    /// Declares the Instrument picker; picking an instrument closes it.
    pub(super) fn show_instrument_picker(&mut self, ctx: &egui::Context) {
        let Some(picker) = &self.instrument_picker else {
            return;
        };
        if self.project.track(picker.track_id).and_then(InstrumentTrack::instrument_def).is_none() {
            self.instrument_picker = None;
            return;
        }
        self.floating_window(
            ctx,
            super::floating::WindowSpec::new(
                "instrument_picker_window",
                "Choose instrument",
                Vec2::new(560.0, 400.0),
                Vec2::new(420.0, 240.0),
            ),
            |app, ui| app.instrument_picker_ui(ui),
            |app, _| app.instrument_picker = None,
        );
    }

    fn instrument_picker_ui(&mut self, ui: &mut egui::Ui) {
        let Some(picker) = &mut self.instrument_picker else {
            return;
        };
        let Some(current) = self
            .project
            .track(picker.track_id)
            .and_then(InstrumentTrack::instrument_def)
            .map(|def| def.id)
        else {
            self.instrument_picker = None;
            return;
        };
        let track_id = picker.track_id;
        if let Some(def) = instrument_list(ui, &mut picker.filter, current) {
            self.instrument_picker = None;
            self.apply_mix_event(MixEvent {
                track_id,
                edit: Some(MixEdit::Instrument(def)),
                commit: true,
            });
        }
    }
}

/// The Sound menu of one track, as a button in the track table. Picks go to
/// `events`. While the track plays with an automatic sound, `playing` is the
/// preset sounding at the playhead and the button shows it.
pub(super) fn sound_menu_button(
    ui: &mut egui::Ui,
    track: &InstrumentTrack,
    playing: Option<(u16, u8)>,
    width: f32,
    events: &mut Vec<MixEvent>,
) {
    let (bank, program) = track.sound();
    let (shown_bank, shown_program) = playing.filter(|_| track.sound_auto).unwrap_or((bank, program));
    let label = if track.sound_auto {
        format!("Auto: {}", sounds::sound_label(shown_bank, shown_program))
    } else {
        sounds::sound_label(bank, program)
    };
    let button = egui::Button::new(label).truncate()
        .min_size(Vec2::new(width - 8.0, 0.0));
    let track_id = track.id;
    let auto = track.sound_auto;
    let mut pick = |events: &mut Vec<MixEvent>, edit: MixEdit| {
        events.push(MixEvent {
            track_id,
            edit: Some(edit),
            commit: true,
        });
    };
    egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        ui.set_min_width(190.0);
        let search_id = egui::Id::new(("sound_search", track_id));
        let mut search: String = ui.data_mut(|data| data.get_temp(search_id).unwrap_or_default());
        let edit = ui.add(
            egui::TextEdit::singleline(&mut search)
                .hint_text("Search sounds")
                .desired_width(180.0),
        );
        if edit.changed() {
            ui.data_mut(|data| data.insert_temp(search_id, search.clone()));
        }
        ui.separator();

        if !search.trim().is_empty() {
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                let found = sounds::search_presets(&search);
                if found.is_empty() {
                    ui.weak("No sound matches.");
                }
                for (found_bank, found_program, name) in found {
                    let current = !auto && (found_bank, found_program) == (bank, program);
                    if ui.selectable_label(current, name).clicked() {
                        pick(events, MixEdit::Sound(found_bank, found_program));
                        ui.close();
                    }
                }
            });
            return;
        }

        ui.add_enabled(
            false,
            egui::Button::selectable(true, sounds::sound_label(shown_bank, shown_program)),
        );
        ui.separator();
        ui.menu_button("SoundFonts", |ui| {
            ui.menu_button("MS Basic", |ui| {
                if ui.selectable_label(auto, "Choose automatically").clicked() {
                    pick(events, MixEdit::SoundAutomatic);
                    ui.close();
                }
                ui.separator();
                for entry in MENU {
                    match entry {
                        SoundMenu::Presets(title, list) => {
                            ui.menu_button(*title, |ui| {
                                preset_items(ui, list, (bank, program), auto, events, &mut pick);
                            });
                        }
                        SoundMenu::Groups(title, groups) => {
                            ui.menu_button(*title, |ui| {
                                for (group, list) in *groups {
                                    ui.menu_button(*group, |ui| {
                                        preset_items(ui, list, (bank, program), auto, events, &mut pick);
                                    });
                                }
                            });
                        }
                    }
                }
            });
        });
    });
}

fn preset_items(
    ui: &mut egui::Ui,
    list: &[(u16, u8)],
    current: (u16, u8),
    auto: bool,
    events: &mut Vec<MixEvent>,
    pick: &mut impl FnMut(&mut Vec<MixEvent>, MixEdit),
) {
    for &(bank, program) in list {
        if !sounds::offered_in_menu(bank, program) {
            continue;
        }
        let selected = !auto && (bank, program) == current;
        if ui
            .selectable_label(selected, sounds::sound_label(bank, program))
            .clicked()
        {
            pick(events, MixEdit::Sound(bank, program));
            ui.close();
        }
    }
}
