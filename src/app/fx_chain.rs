//! The FX Chain window, laid out like Reaper's: a list of the plug-ins in
//! one chain on the left, the picked plug-in on the right. Every FX button
//! (one per track, plus Global FX) opens the chain of its own; a chain
//! starts empty and the "Add" button opens a browser that files the
//! installed plug-ins in a folder per developer.

use std::collections::{BTreeMap, HashMap};

use super::*;
use crate::plugins::host::EditorWindow;

/// Which chain a window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum FxTarget {
    /// The chain on the final mix.
    Master,
    Track(u64),
}

pub(super) struct FxWindow {
    target: FxTarget,
    /// The plug-in shown on the right.
    selected: Option<u64>,
}

/// The "Add FX" browser.
pub(super) struct FxBrowser {
    target: FxTarget,
    filter: String,
    /// Path and class of the picked plug-in.
    picked: Option<(String, String)>,
    /// When set, the pick replaces this slot instead of being added.
    replace: Option<u64>,
}

#[derive(Default)]
pub(super) struct FxChainState {
    windows: Vec<FxWindow>,
    /// The plug-in each chain showed last, so reopening the chain shows it.
    last_opened: HashMap<FxTarget, u64>,
    browser: Option<FxBrowser>,
    /// The editor window each chain has open, with the slot it shows.
    editors: HashMap<FxTarget, (u64, EditorWindow)>,
    /// Why a slot's editor did not open.
    editor_notes: HashMap<u64, String>,
    renaming: Option<(u64, String)>,
}

/// What the chain list asked for during a frame.
enum ChainAction {
    Select(u64),
    Bypass(u64, bool),
    Remove(u64),
    MoveUp(u64),
    MoveDown(u64),
    Replace(u64),
    Rename(u64),
    OpenEditor(u64),
    Retry(u64),
    BypassAll(bool),
}

/// Plug-ins filed by developer, folders in alphabetical order.
pub(super) fn group_by_developer<'a>(
    plugins: impl Iterator<Item = &'a ScannedPlugin>,
    filter: &str,
    allow_instruments: bool,
) -> BTreeMap<String, Vec<&'a ScannedPlugin>> {
    let needle = filter.trim().to_lowercase();
    let mut folders: BTreeMap<String, Vec<&ScannedPlugin>> = BTreeMap::new();
    for plugin in plugins {
        if !allow_instruments && plugin.kind == PluginKind::Instrument {
            continue;
        }
        if !needle.is_empty()
            && !plugin.name.to_lowercase().contains(&needle)
            && !plugin.developer().to_lowercase().contains(&needle)
        {
            continue;
        }
        folders
            .entry(plugin.developer().to_owned())
            .or_default()
            .push(plugin);
    }
    for list in folders.values_mut() {
        list.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.format.cmp(&b.format))
        });
    }
    folders
}

impl BetterWriterApp {
    pub(super) fn fx_chain(&self, target: FxTarget) -> Option<&Vec<SoundChainSlot>> {
        match target {
            FxTarget::Master => Some(&self.project.master_chain),
            FxTarget::Track(id) => self.project.track(id).map(|track| &track.sound_chain),
        }
    }

    fn fx_chain_mut(&mut self, target: FxTarget) -> Option<&mut Vec<SoundChainSlot>> {
        match target {
            FxTarget::Master => Some(&mut self.project.master_chain),
            FxTarget::Track(id) => self.project.track_mut(id).map(|track| &mut track.sound_chain),
        }
    }

    fn fx_bypassed(&self, target: FxTarget) -> bool {
        match target {
            FxTarget::Master => self.project.master_fx_bypassed,
            FxTarget::Track(id) => self.project.track(id).is_some_and(|track| track.fx_bypassed),
        }
    }

    fn set_fx_bypassed(&mut self, target: FxTarget, bypassed: bool) {
        match target {
            FxTarget::Master => self.project.master_fx_bypassed = bypassed,
            FxTarget::Track(id) => {
                if let Some(track) = self.project.track_mut(id) {
                    track.fx_bypassed = bypassed;
                }
            }
        }
    }

    pub(super) fn fx_state(&self, target: FxTarget) -> FxState {
        match self.fx_chain(target) {
            Some(chain) if !chain.is_empty() => {
                if self.fx_bypassed(target) {
                    FxState::Bypassed
                } else {
                    FxState::Active
                }
            }
            _ => FxState::Empty,
        }
    }

    /// Switches a chain's power button as one undo step.
    pub(super) fn toggle_fx_power(&mut self, target: FxTarget) {
        let before = self.project.clone();
        let bypassed = !self.fx_bypassed(target);
        self.set_fx_bypassed(target, bypassed);
        self.record_project_edit(before);
        self.status = if bypassed {
            "Effects bypassed.".to_owned()
        } else {
            "Effects in use.".to_owned()
        };
    }

    fn fx_target_name(&self, target: FxTarget) -> String {
        match target {
            FxTarget::Master => "Global FX".to_owned(),
            FxTarget::Track(id) => self
                .project
                .track(id)
                .map_or_else(|| "Track".to_owned(), |track| track.name.clone()),
        }
    }

    pub(super) fn fx_window_open(&self, target: FxTarget) -> bool {
        self.fx.windows.iter().any(|window| window.target == target)
    }

    /// An FX button opens its chain's window, or closes it if it is open.
    pub(super) fn toggle_fx_window(&mut self, target: FxTarget) {
        if self.fx_window_open(target) {
            self.close_fx_window(target);
            return;
        }
        // The chain opens on the plug-in it showed last.
        let chain_ids: Vec<u64> = self
            .fx_chain(target)
            .map(|chain| chain.iter().map(|slot| slot.id).collect())
            .unwrap_or_default();
        let selected = self
            .fx
            .last_opened
            .get(&target)
            .copied()
            .filter(|id| chain_ids.contains(id))
            .or_else(|| chain_ids.first().copied());
        self.fx.windows.push(FxWindow { target, selected });
        if let Some(id) = selected {
            self.open_fx_editor(target, id);
        }
    }

    fn close_fx_window(&mut self, target: FxTarget) {
        self.fx.windows.retain(|window| window.target != target);
        self.fx.editors.remove(&target);
        if self.fx.browser.as_ref().is_some_and(|browser| browser.target == target) {
            self.fx.browser = None;
        }
    }

    /// Shows the slot's own editor in the chain's editor window, replacing
    /// whatever that window showed.
    fn open_fx_editor(&mut self, target: FxTarget, slot_id: u64) {
        if let Some(window) = self.fx.windows.iter_mut().find(|window| window.target == target) {
            window.selected = Some(slot_id);
        }
        self.fx.last_opened.insert(target, slot_id);
        if self
            .fx
            .editors
            .get(&target)
            .is_some_and(|(shown, window)| *shown == slot_id && window.is_open())
        {
            if let Some((_, window)) = self.fx.editors.get(&target) {
                window.focus();
            }
            return;
        }
        self.fx.editors.remove(&target);
        self.fx.editor_notes.remove(&slot_id);
        let Some(slot) = self
            .fx_chain(target)
            .and_then(|chain| chain.iter().find(|slot| slot.id == slot_id))
            .cloned()
        else {
            return;
        };
        let Some(live) = self.plugin_host.ensure(&slot) else {
            return;
        };
        let title = format!("{} - {}", slot.display_name(), self.fx_target_name(target));
        match EditorWindow::open(&title, &live) {
            Ok(window) => {
                self.fx.editors.insert(target, (slot_id, window));
            }
            Err(note) => {
                self.fx.editor_notes.insert(slot_id, note);
            }
        }
    }

    /// Adds `plugin` to the end of a chain, or in place of `replace`, and
    /// opens it.
    fn add_fx(&mut self, target: FxTarget, plugin: &ScannedPlugin, replace: Option<u64>) {
        let before = self.project.clone();
        let mut slot = plugin.to_slot();
        slot.id = crate::core::fresh_slot_id();
        let slot_id = slot.id;
        let name = slot.name.clone();
        let Some(chain) = self.fx_chain_mut(target) else {
            return;
        };
        match replace.and_then(|id| chain.iter().position(|existing| existing.id == id)) {
            Some(index) => {
                slot.bypassed = chain[index].bypassed;
                chain[index] = slot;
            }
            None => chain.push(slot),
        }
        self.record_project_edit(before);
        self.status = format!("Added {name} to the FX chain.");
        self.open_fx_editor(target, slot_id);
    }

    fn apply_chain_action(&mut self, target: FxTarget, action: ChainAction) {
        match action {
            ChainAction::Select(id) | ChainAction::OpenEditor(id) => self.open_fx_editor(target, id),
            ChainAction::Retry(id) => {
                self.plugin_host.retry(id);
                self.open_fx_editor(target, id);
            }
            ChainAction::Replace(id) => {
                self.fx.browser = Some(FxBrowser {
                    target,
                    filter: String::new(),
                    picked: None,
                    replace: Some(id),
                });
            }
            ChainAction::Rename(id) => {
                let current = self
                    .fx_chain(target)
                    .and_then(|chain| chain.iter().find(|slot| slot.id == id))
                    .map(|slot| slot.display_name().to_owned())
                    .unwrap_or_default();
                self.fx.renaming = Some((id, current));
            }
            ChainAction::BypassAll(bypassed) => {
                let before = self.project.clone();
                self.set_fx_bypassed(target, bypassed);
                self.record_project_edit(before);
            }
            ChainAction::Bypass(id, bypassed) => {
                let before = self.project.clone();
                if let Some(slot) = self
                    .fx_chain_mut(target)
                    .and_then(|chain| chain.iter_mut().find(|slot| slot.id == id))
                {
                    slot.bypassed = bypassed;
                }
                self.record_project_edit(before);
            }
            ChainAction::MoveUp(id) | ChainAction::MoveDown(id) => {
                let up = matches!(action, ChainAction::MoveUp(_));
                let before = self.project.clone();
                if let Some(chain) = self.fx_chain_mut(target)
                    && let Some(index) = chain.iter().position(|slot| slot.id == id)
                {
                    if up && index > 0 {
                        chain.swap(index, index - 1);
                    } else if !up && index + 1 < chain.len() {
                        chain.swap(index, index + 1);
                    }
                }
                self.record_project_edit(before);
            }
            ChainAction::Remove(id) => {
                let before = self.project.clone();
                let mut next = None;
                if let Some(chain) = self.fx_chain_mut(target)
                    && let Some(index) = chain.iter().position(|slot| slot.id == id)
                {
                    chain.remove(index);
                    next = chain
                        .get(index)
                        .or_else(|| index.checked_sub(1).and_then(|prev| chain.get(prev)))
                        .map(|slot| slot.id);
                }
                self.record_project_edit(before);
                if self.fx.editors.get(&target).is_some_and(|(shown, _)| *shown == id) {
                    self.fx.editors.remove(&target);
                }
                if self.fx.last_opened.get(&target) == Some(&id) {
                    self.fx.last_opened.remove(&target);
                }
                if let Some(window) = self.fx.windows.iter_mut().find(|w| w.target == target) {
                    window.selected = next;
                }
                if let Some(next) = next {
                    self.open_fx_editor(target, next);
                }
                self.status = "Removed the plug-in from the FX chain.".to_owned();
            }
        }
    }

    /// Draws every open FX Chain window and the Add FX browser.
    pub(super) fn show_fx_windows(&mut self, ctx: &egui::Context) {
        // A window whose track is gone closes; an editor the user closed
        // forgets itself.
        let gone: Vec<FxTarget> = self
            .fx
            .windows
            .iter()
            .map(|window| window.target)
            .filter(|target| self.fx_chain(*target).is_none())
            .collect();
        for target in gone {
            self.close_fx_window(target);
        }
        for (_, window) in self.fx.editors.values_mut() {
            window.update();
        }
        self.fx.editors.retain(|_, (_, window)| window.is_open());
        if !self.fx.editors.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        let targets: Vec<FxTarget> = self.fx.windows.iter().map(|window| window.target).collect();
        for target in targets {
            self.show_fx_window(ctx, target);
        }
        self.show_fx_browser(ctx);
    }

    fn show_fx_window(&mut self, ctx: &egui::Context, target: FxTarget) {
        let Some(chain) = self.fx_chain(target).cloned() else {
            return;
        };
        let selected = self
            .fx
            .windows
            .iter()
            .find(|window| window.target == target)
            .and_then(|window| window.selected)
            .filter(|id| chain.iter().any(|slot| slot.id == *id));
        let bypassed = self.fx_bypassed(target);
        let title = format!("FX Chain - {}", self.fx_target_name(target));
        let id = egui::Id::new(("fx_chain_window", target));
        let mut open = true;
        let mut action: Option<ChainAction> = None;
        let mut add = false;
        let accent = self.theme.accent;
        let warning = self.theme.warning2_color;
        let muted = self.theme.muted_foreground;

        egui::Window::new(title)
            .id(id)
            .open(&mut open)
            .default_pos([260.0, 120.0])
            .default_size([700.0, 420.0])
            .min_size([460.0, 240.0])
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("FX", |ui| {
                        if ui.button("Add FX...").clicked() {
                            add = true;
                            ui.close();
                        }
                        if ui
                            .add_enabled(selected.is_some(), egui::Button::new("Replace FX..."))
                            .clicked()
                        {
                            action = selected.map(ChainAction::Replace);
                            ui.close();
                        }
                        if ui
                            .add_enabled(selected.is_some(), egui::Button::new("Remove FX"))
                            .clicked()
                        {
                            action = selected.map(ChainAction::Remove);
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(selected.is_some(), egui::Button::new("Open FX window"))
                            .clicked()
                        {
                            action = selected.map(ChainAction::OpenEditor);
                            ui.close();
                        }
                        ui.separator();
                        let label = if bypassed { "Use all FX of this chain" } else { "Bypass all FX of this chain" };
                        if ui.add_enabled(!chain.is_empty(), egui::Button::new(label)).clicked() {
                            action = Some(ChainAction::BypassAll(!bypassed));
                            ui.close();
                        }
                    });
                    ui.menu_button("Options", |ui| {
                        if ui
                            .add_enabled(!self.plugin_scanner.is_scanning(), egui::Button::new("Rescan plug-ins"))
                            .clicked()
                        {
                            self.plugin_scanner.rescan();
                            ui.close();
                        }
                    });
                });
                ui.separator();

                // The body takes whatever height the window has, so
                // stretching the window stretches the list.
                let height = ui.available_height();
                let width = ui.available_width();
                let list_width = 240.0_f32.min(width * 0.45);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(list_width, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_height(height);
                            let buttons_height = ui.spacing().interact_size.y + 8.0;
                            let list_height = (height - buttons_height).max(40.0);
                            egui::Frame::NONE
                                .stroke(Stroke::new(1.0_f32, muted.gamma_multiply(0.5)))
                                .inner_margin(4.0)
                                .show(ui, |ui| {
                                    ui.set_width(list_width - 10.0);
                                    ui.set_min_height(list_height - 10.0);
                                    egui::ScrollArea::vertical()
                                        .id_salt(("fx_list", target))
                                        .max_height(list_height - 10.0)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            self.show_fx_list(
                                                ui, target, &chain, selected, bypassed, &mut action,
                                            );
                                        });
                                });
                            ui.horizontal(|ui| {
                                if ui.button("Add").clicked() {
                                    add = true;
                                }
                                if ui
                                    .add_enabled(selected.is_some(), egui::Button::new("Remove"))
                                    .clicked()
                                {
                                    action = selected.map(ChainAction::Remove);
                                }
                            });
                        },
                    );
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        Vec2::new((ui.available_width()).max(120.0), height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_height(height);
                            self.show_fx_detail(ui, target, &chain, selected, accent, warning, &mut action);
                        },
                    );
                });
            });

        if !open {
            self.close_fx_window(target);
            return;
        }
        if add {
            self.fx.browser = Some(FxBrowser {
                target,
                filter: String::new(),
                picked: None,
                replace: None,
            });
        }
        if let Some(action) = action {
            self.apply_chain_action(target, action);
        }
    }

    fn show_fx_list(
        &mut self,
        ui: &mut egui::Ui,
        target: FxTarget,
        chain: &[SoundChainSlot],
        selected: Option<u64>,
        chain_bypassed: bool,
        action: &mut Option<ChainAction>,
    ) {
        if chain.is_empty() {
            ui.weak("Empty. Press Add to put a plug-in in this chain.");
            return;
        }
        for (index, slot) in chain.iter().enumerate() {
            let renaming = matches!(&self.fx.renaming, Some((id, _)) if *id == slot.id);
            ui.horizontal(|ui| {
                let mut active = !slot.bypassed;
                if ui
                    .checkbox(&mut active, "")
                    .on_hover_text("Active (clear to bypass this plug-in)")
                    .changed()
                {
                    *action = Some(ChainAction::Bypass(slot.id, !active));
                }
                if renaming {
                    if let Some((_, text)) = &mut self.fx.renaming {
                        let edit = ui.add(egui::TextEdit::singleline(text).desired_width(150.0));
                        edit.request_focus();
                        if edit.lost_focus() {
                            let committed = ui.input(|input| !input.key_pressed(egui::Key::Escape));
                            let text = text.trim().to_owned();
                            let id = slot.id;
                            self.fx.renaming = None;
                            if committed {
                                let before = self.project.clone();
                                if let Some(found) = self
                                    .fx_chain_mut(target)
                                    .and_then(|chain| chain.iter_mut().find(|s| s.id == id))
                                {
                                    found.label = if text == found.name { String::new() } else { text };
                                }
                                self.record_project_edit(before);
                            }
                        }
                    }
                    return;
                }
                let mut label = egui::RichText::new(slot.list_label());
                if slot.bypassed || chain_bypassed {
                    label = label.weak().strikethrough();
                }
                let response = ui
                    .selectable_label(selected == Some(slot.id), label)
                    .on_hover_text(&slot.path);
                if response.clicked() {
                    *action = Some(ChainAction::Select(slot.id));
                }
                if response.double_clicked() {
                    *action = Some(ChainAction::OpenEditor(slot.id));
                }
                response.context_menu(|ui| {
                    if ui.button("Open FX window").clicked() {
                        *action = Some(ChainAction::OpenEditor(slot.id));
                        ui.close();
                    }
                    let label = if slot.bypassed { "Use this FX" } else { "Bypass this FX" };
                    if ui.button(label).clicked() {
                        *action = Some(ChainAction::Bypass(slot.id, !slot.bypassed));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Rename FX instance...").clicked() {
                        *action = Some(ChainAction::Rename(slot.id));
                        ui.close();
                    }
                    if ui.button("Replace FX...").clicked() {
                        *action = Some(ChainAction::Replace(slot.id));
                        ui.close();
                    }
                    ui.separator();
                    if ui.add_enabled(index > 0, egui::Button::new("Move up")).clicked() {
                        *action = Some(ChainAction::MoveUp(slot.id));
                        ui.close();
                    }
                    if ui
                        .add_enabled(index + 1 < chain.len(), egui::Button::new("Move down"))
                        .clicked()
                    {
                        *action = Some(ChainAction::MoveDown(slot.id));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Remove FX").clicked() {
                        *action = Some(ChainAction::Remove(slot.id));
                        ui.close();
                    }
                });
            });
        }
    }

    /// The right half: what is known about the picked plug-in, and its
    /// parameters.
    #[allow(clippy::too_many_arguments)]
    fn show_fx_detail(
        &mut self,
        ui: &mut egui::Ui,
        target: FxTarget,
        chain: &[SoundChainSlot],
        selected: Option<u64>,
        _accent: egui::Color32,
        warning: egui::Color32,
        action: &mut Option<ChainAction>,
    ) {
        let Some(slot) = selected.and_then(|id| chain.iter().find(|slot| slot.id == id)) else {
            ui.weak(if chain.is_empty() {
                "Nothing here yet."
            } else {
                "Pick a plug-in on the left."
            });
            return;
        };
        ui.heading(slot.display_name());
        let developer = if slot.vendor.is_empty() { "unknown developer" } else { &slot.vendor };
        ui.label(format!("{}  -  {developer}", slot.list_label().split(':').next().unwrap_or("")));
        ui.weak(&slot.path);
        ui.add_space(4.0);

        let live = self.plugin_host.ensure(slot);
        if live.is_none() {
            let reason = self
                .plugin_host
                .error(slot.id)
                .unwrap_or("the plug-in did not start")
                .to_owned();
            ui.colored_label(warning, reason);
            if ui.button("Try again").clicked() {
                *action = Some(ChainAction::Retry(slot.id));
            }
            return;
        }
        let editor_open = self.fx.editors.get(&target).is_some_and(|(shown, _)| *shown == slot.id);
        ui.horizontal(|ui| {
            let label = if editor_open { "Show editor window" } else { "Open editor window" };
            if ui.button(label).clicked() {
                *action = Some(ChainAction::OpenEditor(slot.id));
            }
        });
        if let Some(note) = self.fx.editor_notes.get(&slot.id) {
            ui.add(egui::Label::new(egui::RichText::new(note).weak()).wrap());
        }
        ui.separator();
        let Some(handle) = live.and_then(|live| live.controller_handle()) else {
            ui.weak("This plug-in has no parameters to show.");
            return;
        };
        let params = handle.parameters();
        if params.is_empty() {
            ui.weak("This plug-in has no parameters to show.");
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt(("fx_params", slot.id))
            .auto_shrink([false, false])
            .show_rows(ui, ui.spacing().interact_size.y + 2.0, params.len(), |ui, range| {
                ui.spacing_mut().slider_width = (ui.available_width() - 260.0).clamp(80.0, 420.0);
                for param in &params[range] {
                    ui.horizontal(|ui| {
                        ui.add_sized([130.0, 18.0], egui::Label::new(&param.title).truncate());
                        let mut value = handle.value(param.id) as f32;
                        let text = handle.text(param.id, value as f64);
                        let slider = egui::Slider::new(&mut value, 0.0..=1.0)
                            .show_value(false)
                            .custom_formatter(|_, _| String::new());
                        let changed = ui.add_enabled(!param.read_only, slider).changed();
                        ui.label(format!("{text} {}", param.units));
                        if changed {
                            handle.set(param.id, value as f64);
                        }
                    });
                }
            });
    }

    fn show_fx_browser(&mut self, ctx: &egui::Context) {
        let Some(browser) = &self.fx.browser else {
            return;
        };
        let target = browser.target;
        let replace = browser.replace;
        let mut filter = browser.filter.clone();
        let mut picked = browser.picked.clone();
        let mut open = true;
        let mut commit: Option<ScannedPlugin> = None;
        let mut cancel = false;
        let mut rescan = false;
        let allow_instruments = target != FxTarget::Master;
        let name = self.fx_target_name(target);
        let accent = self.theme.accent;

        egui::Window::new(if replace.is_some() {
            format!("Replace FX - {name}")
        } else {
            format!("Add FX to {name}")
        })
        .id(egui::Id::new("fx_browser_window"))
        .open(&mut open)
        .default_size([460.0, 480.0])
        .min_size([320.0, 240.0])
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut filter)
                        .hint_text("Search plug-ins or developers")
                        .desired_width(240.0),
                );
                let scanning = self.plugin_scanner.is_scanning();
                if ui
                    .add_enabled(!scanning, egui::Button::new("Rescan"))
                    .on_hover_text("Look for installed plug-ins again")
                    .clicked()
                {
                    rescan = true;
                }
                ui.weak(if scanning {
                    format!("Scanning... {} found", self.plugin_scanner.plugins().len())
                } else {
                    format!("{} found", self.plugin_scanner.plugins().len())
                });
            });
            ui.separator();
            let footer = ui.spacing().interact_size.y + 12.0;
            let list_height = (ui.available_height() - footer).max(60.0);
            egui::ScrollArea::vertical()
                .max_height(list_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let folders = group_by_developer(
                        self.plugin_scanner.plugins().iter(),
                        &filter,
                        allow_instruments,
                    );
                    if folders.is_empty() {
                        ui.weak(if self.plugin_scanner.is_scanning() {
                            "Scanning..."
                        } else {
                            "No plug-ins match."
                        });
                    }
                    let searching = !filter.trim().is_empty();
                    for (developer, plugins) in &folders {
                        egui::CollapsingHeader::new(format!("{developer}  ({})", plugins.len()))
                            .id_salt(("fx_folder", developer))
                            .open(searching.then_some(true))
                            .show(ui, |ui| {
                                for plugin in plugins {
                                    let key = (plugin.path.display().to_string(), plugin.class_id.clone());
                                    let is_picked = picked.as_ref() == Some(&key);
                                    let response = ui
                                        .selectable_label(is_picked, plugin.list_label())
                                        .on_hover_text(plugin.path.display().to_string());
                                    if response.clicked() {
                                        picked = Some(key);
                                    }
                                    if response.double_clicked() {
                                        commit = Some((*plugin).clone());
                                    }
                                }
                            });
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                let chosen = picked.as_ref().and_then(|(path, class)| {
                    self.plugin_scanner
                        .plugins()
                        .iter()
                        .find(|plugin| plugin.path.display().to_string() == *path && plugin.class_id == *class)
                });
                let label = if replace.is_some() { "Replace" } else { "Add" };
                let button = egui::Button::new(
                    egui::RichText::new(label).color(crate::theme::contrast_text(accent)),
                )
                .fill(accent);
                if ui.add_enabled(chosen.is_some(), button).clicked() {
                    commit = chosen.cloned();
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });

        if rescan {
            self.plugin_scanner.rescan();
        }
        if let Some(plugin) = commit {
            self.fx.browser = None;
            self.add_fx(target, &plugin, replace);
        } else if cancel || !open {
            self.fx.browser = None;
        } else if let Some(browser) = &mut self.fx.browser {
            browser.filter = filter;
            browser.picked = picked;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::PluginFormat;

    fn plugin(name: &str, vendor: &str, kind: PluginKind, format: PluginFormat) -> ScannedPlugin {
        ScannedPlugin {
            name: name.to_owned(),
            path: PathBuf::from(format!("/x/{vendor}/{name}")),
            format,
            kind,
            vendor: vendor.to_owned(),
            class_id: String::new(),
        }
    }

    fn sample() -> Vec<ScannedPlugin> {
        vec![
            plugin("Verb", "Zed Audio", PluginKind::Effect, PluginFormat::Vst3),
            plugin("Piano", "Acme", PluginKind::Instrument, PluginFormat::Vst3),
            plugin("Amp", "Acme", PluginKind::Effect, PluginFormat::Clap),
            plugin("Amp", "Acme", PluginKind::Effect, PluginFormat::Vst3),
            plugin("Mystery", "", PluginKind::Effect, PluginFormat::Vst3),
        ]
    }

    #[test]
    fn the_browser_files_plug_ins_by_developer_in_alphabetical_folders() {
        let plugins = sample();
        let folders = group_by_developer(plugins.iter(), "", true);
        let names: Vec<&str> = folders.keys().map(String::as_str).collect();
        assert_eq!(names, ["Acme", "Unknown", "Zed Audio"]);
        let acme: Vec<String> = folders["Acme"].iter().map(|p| p.list_label()).collect();
        assert_eq!(acme, ["VST3: Amp", "CLAP: Amp", "VST3i: Piano"]);
    }

    #[test]
    fn the_search_matches_names_and_developers_and_empty_folders_vanish() {
        let plugins = sample();
        let by_name = group_by_developer(plugins.iter(), "verb", true);
        assert_eq!(by_name.len(), 1);
        let by_developer = group_by_developer(plugins.iter(), "acme", true);
        assert_eq!(by_developer.len(), 1);
        assert_eq!(by_developer["Acme"].len(), 3);
    }

    #[test]
    fn the_global_chain_is_offered_effects_only() {
        let plugins = sample();
        let folders = group_by_developer(plugins.iter(), "", false);
        assert!(folders.values().flatten().all(|plugin| plugin.kind == PluginKind::Effect));
        assert_eq!(folders["Acme"].len(), 2);
    }
}
