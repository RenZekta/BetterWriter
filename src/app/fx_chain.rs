//! The FX Chain window, laid out like Reaper's: a list of the plug-ins in
//! one chain on the left, the picked plug-in on the right. Every FX button
//! (one per track, plus Global FX) opens the chain of its own; a chain
//! starts empty and the "Add" button opens a browser that files the
//! installed plug-ins in a folder per developer.
//!
//! The window is drawn by this file rather than by egui's `Window`, because
//! egui's resizing lets the window drift once it reaches its smallest size
//! or the edge of the program's window. Here every edge is moved on its own
//! from where the drag began, so an edge that cannot move any further simply
//! stops, and the opposite edge never moves.
//!
//! The list takes several selected plug-ins at once (Ctrl and Shift click,
//! Ctrl+A), deletes them with Delete or Backspace, and reorders them by
//! dragging. The right half shows the picked plug-in either as its own
//! editor, laid over the window as a native child window, or as a generic
//! list of parameters; a plug-in can also open in a window of its own.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use super::*;
use crate::plugins::host::{EditorMode, EditorWindow, LivePlugin};
use crate::plugins::native_window;

/// Which chain a window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum FxTarget {
    /// The chain on the final mix.
    Master,
    Track(u64),
}

/// What the right half of an FX Chain window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FxView {
    /// The plug-in's own editor.
    PluginUi,
    /// A slider for each parameter.
    Parameters,
}

impl FxView {
    fn for_this_system() -> Self {
        if cfg!(windows) { FxView::PluginUi } else { FxView::Parameters }
    }
}

pub(super) struct FxWindow {
    target: FxTarget,
    /// The plug-in shown on the right: the one last picked.
    selected: Option<u64>,
    /// Every plug-in picked in the list.
    selection: BTreeSet<u64>,
    /// Where a Shift click's range starts.
    anchor: Option<u64>,
    view: FxView,
    /// Whether the window is a native window of its own (not an egui window
    /// inside the program's); not known before it has been drawn once.
    native: Option<bool>,
    /// Its native window handle; zero until found.
    hwnd: usize,
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

/// Plug-ins being dragged to a new place in their chain.
struct ListDrag {
    target: FxTarget,
    ids: Vec<u64>,
}

/// What the body of a chain's window is drawn from.
#[derive(Clone, Copy)]
struct FxBodyIn<'a> {
    chain: &'a [SoundChainSlot],
    selected: Option<u64>,
    selection: &'a BTreeSet<u64>,
    selected_ids: &'a [u64],
    view: FxView,
    bypassed: bool,
    warning: egui::Color32,
    muted: egui::Color32,
    accent: egui::Color32,
}

/// What the body of a chain's window asks for.
#[derive(Default)]
struct FxBodyOut {
    action: Option<ChainAction>,
    add: bool,
}

/// Where an embedded editor goes this frame.
struct EmbedSpot {
    slot: u64,
    rect: Rect,
    layer: egui::LayerId,
}

#[derive(Default)]
pub(super) struct FxChainState {
    windows: Vec<FxWindow>,
    /// The plug-in each chain showed last, so reopening the chain shows it.
    last_opened: HashMap<FxTarget, u64>,
    browser: Option<FxBrowser>,
    /// The editors that are open: any number in windows of their own, and
    /// at most one per chain laid over its FX Chain window.
    editors: HashMap<(FxTarget, u64), EditorWindow>,
    /// Why a slot's editor did not open.
    editor_notes: HashMap<u64, String>,
    renaming: Option<(u64, String)>,
    drag: Option<ListDrag>,
    embeds: HashMap<FxTarget, EmbedSpot>,
    /// The editor size each window was last made room for.
    fitted: HashMap<(FxTarget, u64), (i32, i32)>,
    /// When each embedded editor was last shown (a counter), to close the
    /// one used longest ago.
    embed_use: HashMap<(FxTarget, u64), u64>,
    use_clock: u64,
    /// Each plug-in's parameter list and when it was read: asking the
    /// plug-in for all of them (hundreds, one call each) every frame is slow.
    parameters: HashMap<u64, (std::time::Instant, Arc<Vec<crate::plugins::vst3::ParameterDesc>>)>,
}

/// What the chain list asked for during a frame.
enum ChainAction {
    /// A click on a row, with the modifier keys it was made with.
    Pick { id: u64, ctrl: bool, shift: bool },
    SelectAll,
    Bypass(Vec<u64>, bool),
    Remove(Vec<u64>),
    /// Moves the plug-ins to the place before row `to` of the list.
    Reorder { ids: Vec<u64>, to: usize },
    MoveUp(u64),
    MoveDown(u64),
    Replace(u64),
    Rename(u64),
    OpenSeparate(u64),
    ShowHere(u64),
    Retry(u64),
    BypassAll(bool),
    SetView(FxView),
}

/// How many editors of one chain stay alive inside its window.
const EMBED_CACHE: usize = 4;
const FX_MIN_SIZE: Vec2 = Vec2::new(460.0, 240.0);
const FX_DEFAULT_SIZE: Vec2 = Vec2::new(700.0, 420.0);

/// The row a dragged plug-in would go before: the number of rows whose
/// middle is above the pointer.
fn insertion_index(row_middles: &[f32], pointer_y: f32) -> usize {
    row_middles.iter().filter(|middle| **middle < pointer_y).count()
}

/// The chain's ids with `moving` taken out (keeping their order) and put
/// back before the row `to` of the original list.
fn reordered(chain: &[u64], moving: &[u64], to: usize) -> Vec<u64> {
    let mut rest = Vec::with_capacity(chain.len());
    let mut taken = Vec::new();
    let mut at = None;
    for (index, id) in chain.iter().enumerate() {
        if index == to {
            at = Some(rest.len());
        }
        if moving.contains(id) {
            taken.push(*id);
        } else {
            rest.push(*id);
        }
    }
    let at = at.unwrap_or(rest.len());
    rest.splice(at..at, taken);
    rest
}

/// The ids from `a` to `b` in chain order, both included.
fn range_between(chain: &[u64], a: u64, b: u64) -> Vec<u64> {
    let (Some(first), Some(second)) = (
        chain.iter().position(|id| *id == a),
        chain.iter().position(|id| *id == b),
    ) else {
        return vec![b];
    };
    chain[first.min(second)..=first.max(second)].to_vec()
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
        self.fx.windows.push(FxWindow {
            target,
            selected,
            selection: selected.into_iter().collect(),
            anchor: selected,
            view: FxView::for_this_system(),
            native: None,
            hwnd: 0,
        });
    }

    /// Closes the editors of plug-ins no chain holds any more (an undo or a
    /// redo took them out) and forgets what was kept for them, so a slot
    /// that comes back starts from scratch.
    fn forget_vanished_slots(&mut self) {
        let present: HashSet<(FxTarget, u64)> = self
            .fx
            .editors
            .keys()
            .chain(self.fx.fitted.keys())
            .chain(self.fx.embed_use.keys())
            .copied()
            .filter(|(target, id)| {
                self.fx_chain(*target).is_some_and(|chain| chain.iter().any(|slot| slot.id == *id))
            })
            .collect();
        self.fx.editors.retain(|key, _| present.contains(key));
        self.fx.fitted.retain(|key, _| present.contains(key));
        self.fx.embed_use.retain(|key, _| present.contains(key));
        let wanted: HashSet<u64> = self.fx.windows.iter().filter_map(|window| window.selected).collect();
        self.fx.parameters.retain(|id, _| wanted.contains(id));
        let ids: HashSet<u64> = present.iter().map(|(_, id)| *id).collect();
        let chains: Vec<Vec<u64>> = self
            .fx
            .windows
            .iter()
            .filter_map(|window| self.fx_chain(window.target))
            .map(|chain| chain.iter().map(|slot| slot.id).collect())
            .collect();
        self.fx
            .editor_notes
            .retain(|id, _| ids.contains(id) || chains.iter().any(|chain| chain.contains(id)));
    }

    fn close_fx_window(&mut self, target: FxTarget) {
        self.fx.windows.retain(|window| window.target != target);
        self.fx.editors.retain(|(shown, _), _| *shown != target);
        self.fx.embed_use.retain(|(shown, _), _| *shown != target);
        self.fx.embeds.remove(&target);
        self.fx.fitted.retain(|(shown, _), _| *shown != target);
        if self.fx.browser.as_ref().is_some_and(|browser| browser.target == target) {
            self.fx.browser = None;
        }
    }

    /// Makes one plug-in the only one picked.
    fn fx_select_only(&mut self, target: FxTarget, slot_id: u64) {
        if let Some(window) = self.fx.windows.iter_mut().find(|window| window.target == target) {
            window.selected = Some(slot_id);
            window.selection = BTreeSet::from([slot_id]);
            window.anchor = Some(slot_id);
        }
        self.fx.last_opened.insert(target, slot_id);
    }

    /// The slot and its running plug-in, started if need be.
    fn fx_live_slot(&mut self, target: FxTarget, slot_id: u64) -> Option<(SoundChainSlot, Arc<LivePlugin>)> {
        let slot = self
            .fx_chain(target)
            .and_then(|chain| chain.iter().find(|slot| slot.id == slot_id))
            .cloned()?;
        let live = self.plugin_host.ensure(&slot)?;
        Some((slot, live))
    }

    /// Opens the slot's own editor in a window of its own, or brings that
    /// window forward when it is open already.
    fn open_fx_separate(&mut self, target: FxTarget, slot_id: u64) {
        self.fx.last_opened.insert(target, slot_id);
        let key = (target, slot_id);
        if let Some(editor) = self.fx.editors.get(&key)
            && editor.mode() == EditorMode::Floating
            && editor.is_open()
        {
            editor.focus();
            return;
        }
        // One plug-in has one editor: the copy laid over the FX Chain window
        // makes way for the window of its own.
        self.fx.editors.remove(&key);
        self.fx.editor_notes.remove(&slot_id);
        let Some((slot, live)) = self.fx_live_slot(target, slot_id) else {
            return;
        };
        let title = format!("{} - {}", slot.display_name(), self.fx_target_name(target));
        match EditorWindow::open(&title, &live, self.main_window as *mut std::ffi::c_void) {
            Ok(window) => {
                self.fx.editors.insert(key, window);
            }
            Err(note) => {
                self.fx.editor_notes.insert(slot_id, note);
            }
        }
    }

    /// Shows the slot's own editor inside the FX Chain window. Editors that
    /// were shown before stay alive, hidden, so coming back to one is
    /// instant and it does not start from a blank area; the ones used
    /// longest ago are closed beyond `EMBED_CACHE` per chain.
    fn open_fx_embedded(&mut self, target: FxTarget, slot_id: u64) {
        let key = (target, slot_id);
        self.fx.use_clock += 1;
        self.fx.embed_use.insert(key, self.fx.use_clock);
        let alive = self
            .fx
            .editors
            .get(&key)
            .is_some_and(|editor| editor.mode() == EditorMode::Embedded && editor.is_open());
        if !alive {
            self.fx.editors.remove(&key);
            self.fx.editor_notes.remove(&slot_id);
            self.create_fx_embedded(target, slot_id);
        }
        // Close the editors used longest ago.
        loop {
            let embedded: Vec<(FxTarget, u64)> = self
                .fx
                .editors
                .iter()
                .filter(|((shown, _), editor)| *shown == target && editor.mode() == EditorMode::Embedded)
                .map(|(key, _)| *key)
                .collect();
            if embedded.len() <= EMBED_CACHE {
                break;
            }
            let Some(oldest) = embedded
                .into_iter()
                .filter(|other| *other != key)
                .min_by_key(|other| self.fx.embed_use.get(other).copied().unwrap_or(0))
            else {
                break;
            };
            self.fx.editors.remove(&oldest);
            self.fx.embed_use.remove(&oldest);
        }
    }

    /// Makes a slot's editor as a hidden child of the chain's window.
    fn create_fx_embedded(&mut self, target: FxTarget, slot_id: u64) {
        let parent = self.fx_parent_window(target);
        if parent == 0 {
            // The native window is made the first time it is drawn and found
            // right after; nothing is wrong yet.
            if self.fx.windows.iter().any(|window| window.target == target && window.native == Some(false)) {
                self.fx.editor_notes.insert(
                    slot_id,
                    "The program's window is not known here, so the editor cannot be shown inside it; use Parameters or open it in a separate window".to_owned(),
                );
            }
            return;
        }
        let Some((_, live)) = self.fx_live_slot(target, slot_id) else {
            return;
        };
        match EditorWindow::open_embedded(parent as *mut std::ffi::c_void, &live) {
            Ok(window) => {
                self.fx.editors.insert((target, slot_id), window);
            }
            Err(note) => {
                self.fx.editor_notes.insert(slot_id, note);
            }
        }
    }

    /// While nothing plays, makes the editors of the other plug-ins of a
    /// chain in the background, one a frame, so that picking one shows a
    /// finished editor instead of a blank area. A VST3 editor can only be
    /// made on the thread that owns the window, so this is spread over
    /// frames instead of moved to another thread.
    fn preload_fx_editors(&mut self, ctx: &egui::Context) {
        if self.playback_tick.is_some() {
            return;
        }
        let wanted: Vec<(FxTarget, Vec<u64>)> = self
            .fx
            .windows
            .iter()
            .filter(|window| window.view == FxView::PluginUi)
            .filter_map(|window| {
                let chain = self.fx_chain(window.target)?;
                Some((window.target, chain.iter().map(|slot| slot.id).collect()))
            })
            .collect();
        for (target, ids) in wanted {
            let have = self
                .fx
                .editors
                .iter()
                .filter(|((shown, _), editor)| *shown == target && editor.mode() == EditorMode::Embedded)
                .count();
            if have >= EMBED_CACHE {
                continue;
            }
            let next = ids.iter().copied().find(|id| {
                !self.fx.editors.contains_key(&(target, *id)) && !self.fx.editor_notes.contains_key(id)
            });
            if let Some(id) = next {
                self.create_fx_embedded(target, id);
                ctx.request_repaint();
                return;
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
        self.fx_select_only(target, slot_id);
    }

    fn apply_chain_action(&mut self, target: FxTarget, action: ChainAction) {
        match action {
            ChainAction::Pick { id, ctrl, shift } => {
                let ids: Vec<u64> = self
                    .fx_chain(target)
                    .map(|chain| chain.iter().map(|slot| slot.id).collect())
                    .unwrap_or_default();
                let Some(window) = self.fx.windows.iter_mut().find(|window| window.target == target) else {
                    return;
                };
                if shift {
                    let anchor = window.anchor.or(window.selected).unwrap_or(id);
                    if !ctrl {
                        window.selection.clear();
                    }
                    window.selection.extend(range_between(&ids, anchor, id));
                    window.selected = Some(id);
                } else if ctrl {
                    if window.selection.remove(&id) {
                        if window.selected == Some(id) {
                            window.selected = window.selection.iter().next_back().copied();
                        }
                    } else {
                        window.selection.insert(id);
                        window.selected = Some(id);
                    }
                    window.anchor = Some(id);
                } else {
                    window.selection = BTreeSet::from([id]);
                    window.selected = Some(id);
                    window.anchor = Some(id);
                }
                if let Some(shown) = window.selected {
                    self.fx.last_opened.insert(target, shown);
                }
            }
            ChainAction::SelectAll => {
                let ids: Vec<u64> = self
                    .fx_chain(target)
                    .map(|chain| chain.iter().map(|slot| slot.id).collect())
                    .unwrap_or_default();
                if let Some(window) = self.fx.windows.iter_mut().find(|window| window.target == target) {
                    window.selection = ids.iter().copied().collect();
                    window.selected = window.selected.or_else(|| ids.first().copied());
                }
            }
            ChainAction::SetView(view) => {
                if let Some(window) = self.fx.windows.iter_mut().find(|window| window.target == target) {
                    window.view = view;
                }
                if view == FxView::Parameters {
                    // The editors laid over the window are only hidden (no
                    // room is reserved for them any more), so coming back is
                    // instant.
                    self.fx.embeds.remove(&target);
                }
            }
            ChainAction::OpenSeparate(id) => {
                self.fx_select_only(target, id);
                self.open_fx_separate(target, id);
            }
            ChainAction::ShowHere(id) => {
                if let Some(window) = self.fx.windows.iter_mut().find(|window| window.target == target) {
                    window.view = FxView::PluginUi;
                }
                self.open_fx_embedded(target, id);
            }
            ChainAction::Retry(id) => {
                self.plugin_host.retry(id);
                self.fx.editor_notes.remove(&id);
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
            ChainAction::Bypass(ids, bypassed) => {
                let before = self.project.clone();
                if let Some(chain) = self.fx_chain_mut(target) {
                    for slot in chain.iter_mut().filter(|slot| ids.contains(&slot.id)) {
                        slot.bypassed = bypassed;
                    }
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
            ChainAction::Reorder { ids, to } => {
                let before = self.project.clone();
                let Some(chain) = self.fx_chain_mut(target) else {
                    return;
                };
                let old: Vec<u64> = chain.iter().map(|slot| slot.id).collect();
                let new = reordered(&old, &ids, to);
                if new == old {
                    return;
                }
                let mut slots: HashMap<u64, SoundChainSlot> = chain.drain(..).map(|slot| (slot.id, slot)).collect();
                chain.extend(new.iter().filter_map(|id| slots.remove(id)));
                self.record_project_edit(before);
                self.status = if ids.len() == 1 {
                    "Moved the plug-in in the FX chain.".to_owned()
                } else {
                    format!("Moved {} plug-ins in the FX chain.", ids.len())
                };
            }
            ChainAction::Remove(ids) => {
                if ids.is_empty() {
                    return;
                }
                let before = self.project.clone();
                let mut next = None;
                let mut removed = 0;
                if let Some(chain) = self.fx_chain_mut(target)
                    && let Some(index) = chain.iter().position(|slot| ids.contains(&slot.id))
                {
                    let count = chain.len();
                    chain.retain(|slot| !ids.contains(&slot.id));
                    removed = count - chain.len();
                    next = chain
                        .get(index)
                        .or_else(|| index.checked_sub(1).and_then(|prev| chain.get(prev)))
                        .or_else(|| chain.last())
                        .map(|slot| slot.id);
                }
                if removed == 0 {
                    return;
                }
                self.record_project_edit(before);
                self.fx.editors.retain(|(shown, id), _| !(*shown == target && ids.contains(id)));
                if self.fx.last_opened.get(&target).is_some_and(|id| ids.contains(id)) {
                    self.fx.last_opened.remove(&target);
                }
                match next {
                    Some(next) => self.fx_select_only(target, next),
                    None => {
                        if let Some(window) = self.fx.windows.iter_mut().find(|w| w.target == target) {
                            window.selected = None;
                            window.selection.clear();
                            window.anchor = None;
                        }
                    }
                }
                self.status = if removed == 1 {
                    "Removed the plug-in from the FX chain.".to_owned()
                } else {
                    format!("Removed {removed} plug-ins from the FX chain.")
                };
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
        self.forget_vanished_slots();
        for editor in self.fx.editors.values_mut() {
            editor.update();
        }
        self.fx.editors.retain(|_, editor| editor.is_open());
        if !self.fx.editors.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        self.preload_fx_editors(ctx);
        let targets: Vec<FxTarget> = self.fx.windows.iter().map(|window| window.target).collect();
        for target in targets {
            self.show_fx_window(ctx, target);
        }
        self.show_fx_browser(ctx);
    }

    /// Lays each embedded editor of a chain over the place its window
    /// reserved for it, or hides it when something of this program covers
    /// that place. `parent` is the native window the editors are children of.
    fn place_embedded_editors(&mut self, ctx: &egui::Context, target: FxTarget, parent: usize, native: bool) {
        let points = ctx.pixels_per_point();
        let pressed_at = ctx
            .input(|input| input.pointer.any_pressed().then(|| input.pointer.interact_pos()))
            .flatten();
        let mut grow = Vec2::ZERO;
        for ((shown, slot), editor) in self.fx.editors.iter_mut() {
            if *shown != target || editor.mode() != EditorMode::Embedded {
                continue;
            }
            let spot = self.fx.embeds.get(&target).filter(|spot| spot.slot == *slot);
            let Some(spot) = spot else {
                editor.place(0, 0, 1, 1, false);
                continue;
            };
            let rect = spot.rect;
            let visible = rect.width() > 8.0
                && rect.height() > 8.0
                && [
                    rect.center(),
                    rect.min + Vec2::splat(4.0),
                    rect.max - Vec2::splat(4.0),
                    Pos2::new(rect.max.x - 4.0, rect.min.y + 4.0),
                    Pos2::new(rect.min.x + 4.0, rect.max.y - 4.0),
                ]
                .iter()
                .all(|point| ctx.layer_id_at(*point) == Some(spot.layer));
            editor.place(
                (rect.min.x * points).round() as i32,
                (rect.min.y * points).round() as i32,
                (rect.width() * points).round() as i32,
                (rect.height() * points).round() as i32,
                visible,
            );
            // A click anywhere else in the window takes the keyboard back
            // from the editor, which would otherwise keep every keystroke.
            if let Some(pos) = pressed_at
                && !rect.contains(pos)
                && editor.has_keyboard()
            {
                native_window::give_keyboard_to(parent as *mut std::ffi::c_void);
            }
            // An editor that cannot be resized gets a window big enough.
            let wanted = editor.wanted_size();
            if !editor.can_resize() && self.fx.fitted.get(&(*shown, *slot)) != Some(&wanted) {
                self.fx.fitted.insert((*shown, *slot), wanted);
                let wanted_points = Vec2::new(wanted.0 as f32 / points, wanted.1 as f32 / points);
                grow = grow.max((wanted_points - rect.size()).max(Vec2::ZERO));
            }
        }
        if native && (grow.x > 0.5 || grow.y > 0.5) {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(ctx.content_rect().size() + grow));
            ctx.request_repaint();
        }
    }

    /// The title of a chain's window, which is also how its native window is
    /// found; two chains with the same name are told apart by their number.
    fn fx_window_title(&self, target: FxTarget) -> String {
        let name = self.fx_target_name(target);
        let twins = self
            .fx
            .windows
            .iter()
            .filter(|window| self.fx_target_name(window.target) == name)
            .count();
        match target {
            FxTarget::Track(id) if twins > 1 => format!("FX Chain - {name} ({id})"),
            _ => format!("FX Chain - {name}"),
        }
    }

    /// The window that editors laid over this chain's window are children of:
    /// its own native window, or the program's window when the chain is
    /// shown inside it. Zero while the native window is not found yet.
    fn fx_parent_window(&self, target: FxTarget) -> usize {
        match self.fx.windows.iter().find(|window| window.target == target) {
            Some(window) => match window.native {
                Some(true) => window.hwnd,
                Some(false) => self.main_window,
                None => 0,
            },
            None => 0,
        }
    }

    /// Declares a chain's window: a window of the operating system, which can
    /// go anywhere on the desktop (over the program's window, and past it),
    /// or, where the platform has no such windows, an egui window. It draws
    /// itself, see `fx_window_ui`.
    fn show_fx_window(&mut self, ctx: &egui::Context, target: FxTarget) {
        if self.fx_chain(target).is_none() || !self.fx.windows.iter().any(|window| window.target == target) {
            return;
        }
        let title = self.fx_window_title(target);
        self.floating_window(
            ctx,
            super::floating::WindowSpec::new(("fx_chain_viewport", target), &title, FX_DEFAULT_SIZE, FX_MIN_SIZE),
            move |app, ui| app.fx_window_ui(ui, target),
            move |app, _| app.close_fx_window(target),
        );
    }

    /// Draws a chain's window in its own frame. Owned by the program's
    /// window, it stays above it, like a plug-in window in a DAW.
    fn fx_window_ui(&mut self, ui: &mut egui::Ui, target: FxTarget) {
        let Some(chain) = self
            .fx_chain(target)
            .map(|chain| chain.iter().map(SoundChainSlot::without_state).collect::<Vec<_>>())
        else {
            return;
        };
        let Some(index) = self.fx.windows.iter().position(|window| window.target == target) else {
            return;
        };
        let chain_ids: Vec<u64> = chain.iter().map(|slot| slot.id).collect();
        {
            let window = &mut self.fx.windows[index];
            window.selection.retain(|id| chain_ids.contains(id));
            if window.selected.is_none_or(|id| !chain_ids.contains(&id)) {
                window.selected = window.selection.iter().next().copied();
            }
        }
        let selected = self.fx.windows[index].selected;
        let selection = self.fx.windows[index].selection.clone();
        let view = self.fx.windows[index].view;
        let selected_ids: Vec<u64> = chain_ids.iter().copied().filter(|id| selection.contains(id)).collect();
        let input = FxBodyIn {
            chain: &chain,
            selected,
            selection: &selection,
            selected_ids: &selected_ids,
            view,
            bypassed: self.fx_bypassed(target),
            warning: self.theme.warning2_color,
            muted: self.theme.muted_foreground,
            accent: self.theme.accent,
        };
        let native = !ui.ctx().embed_viewports();
        let hwnd = self.native_windows.get(&ui.ctx().viewport_id()).copied().unwrap_or(0);
        let mut out = FxBodyOut::default();
        self.fx.embeds.remove(&target);

        self.show_fx_body(ui, target, &input, &mut out);
        // Delete and Backspace remove the picked plug-ins, Ctrl+A picks them
        // all, while this window has the keyboard.
        if !ui.ctx().text_edit_focused() && self.fx.renaming.is_none() {
            let (remove, all) = ui.input_mut(|state| {
                let delete = state.consume_key(egui::Modifiers::NONE, egui::Key::Delete);
                let backspace = state.consume_key(egui::Modifiers::NONE, egui::Key::Backspace);
                let all = state.consume_key(egui::Modifiers::COMMAND, egui::Key::A);
                (delete || backspace, all)
            });
            if remove && !selected_ids.is_empty() {
                out.action = Some(ChainAction::Remove(selected_ids.to_vec()));
            } else if all {
                out.action = Some(ChainAction::SelectAll);
            }
        }
        let parent = if native { hwnd } else { self.main_window };
        self.place_embedded_editors(ui.ctx(), target, parent, native);

        if let Some(window) = self.fx.windows.iter_mut().find(|window| window.target == target) {
            window.native = Some(native);
            window.hwnd = hwnd;
        }
        if out.add {
            self.fx.browser = Some(FxBrowser {
                target,
                filter: String::new(),
                picked: None,
                replace: None,
            });
        }
        if let Some(action) = out.action {
            self.apply_chain_action(target, action);
        }
    }

    /// The inside of a chain's window: the menu bar, the list of plug-ins on
    /// the left and the picked plug-in on the right.
    fn show_fx_body(&mut self, ui: &mut egui::Ui, target: FxTarget, input: &FxBodyIn, out: &mut FxBodyOut) {
        let FxBodyIn {
            chain,
            selected,
            selection,
            selected_ids,
            view,
            bypassed,
            warning,
            muted,
            accent,
        } = *input;
        let layer = ui.layer_id();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("FX", |ui| {
                if ui.button("Add FX...").clicked() {
                    out.add = true;
                    ui.close();
                }
                if ui
                    .add_enabled(selected.is_some(), egui::Button::new("Replace FX..."))
                    .clicked()
                {
                    out.action = selected.map(ChainAction::Replace);
                    ui.close();
                }
                if ui
                    .add_enabled(!selected_ids.is_empty(), egui::Button::new("Remove FX"))
                    .clicked()
                {
                    out.action = Some(ChainAction::Remove(selected_ids.to_vec()));
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(selected.is_some(), egui::Button::new("Open in a separate window"))
                    .clicked()
                {
                    out.action = selected.map(ChainAction::OpenSeparate);
                    ui.close();
                }
                ui.separator();
                let label = if bypassed { "Use all FX of this chain" } else { "Bypass all FX of this chain" };
                if ui.add_enabled(!chain.is_empty(), egui::Button::new(label)).clicked() {
                    out.action = Some(ChainAction::BypassAll(!bypassed));
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
            // The switch for the right half sits at the right
            // end of the menu bar.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .selectable_label(view == FxView::Parameters, "Parameters")
                    .on_hover_text("Show the plug-in's parameters as sliders")
                    .clicked()
                {
                    out.action = Some(ChainAction::SetView(FxView::Parameters));
                }
                if ui
                    .add_enabled(
                        cfg!(windows),
                        egui::Button::selectable(view == FxView::PluginUi, "Plug-in UI"),
                    )
                    .on_hover_text("Show the plug-in's own editor here (Windows)")
                    .clicked()
                {
                    out.action = Some(ChainAction::SetView(FxView::PluginUi));
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
                                        ui, target, &chain, &selection, bypassed, accent, &mut out.action,
                                    );
                                });
                        });
                    ui.horizontal(|ui| {
                        if ui.button("Add").clicked() {
                            out.add = true;
                        }
                        if ui
                            .add_enabled(!selected_ids.is_empty(), egui::Button::new("Remove"))
                            .clicked()
                        {
                            out.action = Some(ChainAction::Remove(selected_ids.to_vec()));
                        }
                    });
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width().max(120.0), height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_min_height(height);
                    self.show_fx_detail(
                        ui,
                        target,
                        &chain,
                        selected,
                        selection.len(),
                        view,
                        layer,
                        warning,
                        &mut out.action,
                    );
                },
            );
        });    }

    #[allow(clippy::too_many_arguments)]
    fn show_fx_list(
        &mut self,
        ui: &mut egui::Ui,
        target: FxTarget,
        chain: &[SoundChainSlot],
        selection: &BTreeSet<u64>,
        chain_bypassed: bool,
        accent: egui::Color32,
        action: &mut Option<ChainAction>,
    ) {
        if chain.is_empty() {
            ui.weak("Empty. Press Add to put a plug-in in this chain.");
            return;
        }
        let list_rect = ui.clip_rect();
        let ids: Vec<u64> = chain.iter().map(|slot| slot.id).collect();
        // The plug-ins an action on a row means: all picked ones when the
        // row is one of several picked, otherwise the row alone.
        let acts_on = |id: u64| -> Vec<u64> {
            if selection.len() > 1 && selection.contains(&id) {
                ids.iter().copied().filter(|other| selection.contains(other)).collect()
            } else {
                vec![id]
            }
        };
        let mut rows: Vec<Rect> = Vec::with_capacity(chain.len());
        for (index, slot) in chain.iter().enumerate() {
            let renaming = matches!(&self.fx.renaming, Some((id, _)) if *id == slot.id);
            let row_top = ui.cursor().top();
            ui.horizontal(|ui| {
                let mut active = !slot.bypassed;
                if ui
                    .checkbox(&mut active, "")
                    .on_hover_text("Active (clear to bypass this plug-in)")
                    .changed()
                {
                    *action = Some(ChainAction::Bypass(acts_on(slot.id), !active));
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
                let picked = selection.contains(&slot.id);
                let mut label = egui::RichText::new(slot.list_label());
                if slot.bypassed || chain_bypassed {
                    label = label.weak().strikethrough();
                }
                let shown = ui.add(
                    egui::Button::selectable(picked, label)
                        .min_size(Vec2::new(ui.available_width(), 0.0)),
                );
                // One widget over the row takes the click, the double click
                // and the drag.
                let response = ui
                    .interact(shown.rect, shown.id.with("row"), Sense::CLICK | Sense::DRAG)
                    .on_hover_text(&slot.path);
                if response.clicked() {
                    let modifiers = ui.input(|input| input.modifiers);
                    *action = Some(ChainAction::Pick {
                        id: slot.id,
                        ctrl: modifiers.command,
                        shift: modifiers.shift,
                    });
                }
                if response.double_clicked() {
                    *action = Some(ChainAction::OpenSeparate(slot.id));
                }
                if response.secondary_clicked() && !picked {
                    *action = Some(ChainAction::Pick { id: slot.id, ctrl: false, shift: false });
                }
                if response.drag_started() {
                    let moving = acts_on(slot.id);
                    if !picked {
                        *action = Some(ChainAction::Pick { id: slot.id, ctrl: false, shift: false });
                    }
                    self.fx.drag = Some(ListDrag { target, ids: moving });
                }
                response.context_menu(|ui| {
                    let affected = acts_on(slot.id);
                    let several = affected.len() > 1;
                    if ui.button("Open in a separate window").clicked() {
                        *action = Some(ChainAction::OpenSeparate(slot.id));
                        ui.close();
                    }
                    if ui.button("Show in the FX Chain window").clicked() {
                        *action = Some(ChainAction::ShowHere(slot.id));
                        ui.close();
                    }
                    let label = match (slot.bypassed, several) {
                        (true, false) => "Use this FX",
                        (false, false) => "Bypass this FX",
                        (true, true) => "Use these FX",
                        (false, true) => "Bypass these FX",
                    };
                    if ui.button(label).clicked() {
                        *action = Some(ChainAction::Bypass(affected.clone(), !slot.bypassed));
                        ui.close();
                    }
                    ui.separator();
                    if ui.add_enabled(!several, egui::Button::new("Rename FX instance...")).clicked() {
                        *action = Some(ChainAction::Rename(slot.id));
                        ui.close();
                    }
                    if ui.add_enabled(!several, egui::Button::new("Replace FX...")).clicked() {
                        *action = Some(ChainAction::Replace(slot.id));
                        ui.close();
                    }
                    ui.separator();
                    if ui.add_enabled(!several && index > 0, egui::Button::new("Move up")).clicked() {
                        *action = Some(ChainAction::MoveUp(slot.id));
                        ui.close();
                    }
                    if ui
                        .add_enabled(!several && index + 1 < chain.len(), egui::Button::new("Move down"))
                        .clicked()
                    {
                        *action = Some(ChainAction::MoveDown(slot.id));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(if several { "Remove these FX" } else { "Remove FX" }).clicked() {
                        *action = Some(ChainAction::Remove(affected));
                        ui.close();
                    }
                });
            });
            let row_bottom = ui.min_rect().bottom().max(ui.cursor().top() - ui.spacing().item_spacing.y);
            rows.push(Rect::from_min_max(
                Pos2::new(list_rect.min.x, row_top),
                Pos2::new(list_rect.max.x, row_bottom.max(row_top + 1.0)),
            ));
        }
        self.finish_list_drag(ui, target, &rows, list_rect, accent, action);
    }

    /// Shows where a dragged plug-in would land, and moves it on release.
    fn finish_list_drag(
        &mut self,
        ui: &mut egui::Ui,
        target: FxTarget,
        rows: &[Rect],
        list_rect: Rect,
        accent: egui::Color32,
        action: &mut Option<ChainAction>,
    ) {
        let Some(drag) = self.fx.drag.as_ref().filter(|drag| drag.target == target) else {
            return;
        };
        let (pointer, released, cancelled) = ui.input(|input| {
            (
                input.pointer.interact_pos(),
                input.pointer.any_released(),
                input.key_pressed(egui::Key::Escape),
            )
        });
        let Some(pointer) = pointer else {
            self.fx.drag = None;
            return;
        };
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let middles: Vec<f32> = rows.iter().map(|row| row.center().y).collect();
        let to = insertion_index(&middles, pointer.y);
        let inside = list_rect.contains(pointer);
        if inside {
            let y = match to {
                0 => rows.first().map_or(list_rect.min.y, |row| row.top()),
                _ => rows.get(to - 1).map_or(list_rect.max.y, |row| row.bottom()),
            };
            ui.painter().hline(list_rect.x_range(), y, Stroke::new(2.0_f32, accent));
        }
        // A small label follows the pointer.
        let label = if drag.ids.len() == 1 {
            self.fx_chain(target)
                .and_then(|chain| chain.iter().find(|slot| slot.id == drag.ids[0]))
                .map_or_else(|| "Plug-in".to_owned(), |slot| slot.list_label())
        } else {
            format!("{} plug-ins", drag.ids.len())
        };
        let ghost = ui
            .ctx()
            .layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("fx_drag_ghost")));
        let galley = ghost.layout_no_wrap(label, FontId::proportional(14.0), ui.visuals().text_color());
        let at = pointer + Vec2::new(12.0, 8.0);
        ghost.rect_filled(
            Rect::from_min_size(at, galley.size()).expand(3.0),
            3.0,
            ui.visuals().window_fill().gamma_multiply(0.95),
        );
        ghost.galley(at, galley, ui.visuals().text_color());
        ui.ctx().request_repaint();

        if cancelled || (released && !inside) {
            self.fx.drag = None;
        } else if released {
            let ids = self.fx.drag.take().map(|drag| drag.ids).unwrap_or_default();
            *action = Some(ChainAction::Reorder { ids, to });
        }
    }

    /// The right half: what is known about the picked plug-in, and its
    /// editor or parameters.
    #[allow(clippy::too_many_arguments)]
    fn show_fx_detail(
        &mut self,
        ui: &mut egui::Ui,
        target: FxTarget,
        chain: &[SoundChainSlot],
        selected: Option<u64>,
        picked: usize,
        view: FxView,
        layer: egui::LayerId,
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
        let extra = if picked > 1 { format!("  -  {picked} picked") } else { String::new() };
        ui.label(format!(
            "{}  -  {developer}{extra}",
            slot.list_label().split(':').next().unwrap_or("")
        ));
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
        ui.separator();
        match view {
            FxView::PluginUi => self.show_fx_editor_pane(ui, target, slot.id, layer, action),
            FxView::Parameters => {
                if let Some(note) = self.fx.editor_notes.get(&slot.id) {
                    ui.add(egui::Label::new(egui::RichText::new(note).weak()).wrap());
                }
                self.show_fx_parameters(ui, slot.id, live);
            }
        }
    }

    /// The right half in the plug-in UI view: the room the native editor is
    /// laid over, or the reason there is none.
    fn show_fx_editor_pane(
        &mut self,
        ui: &mut egui::Ui,
        target: FxTarget,
        slot_id: u64,
        layer: egui::LayerId,
        action: &mut Option<ChainAction>,
    ) {
        match self.fx.editors.get(&(target, slot_id)).map(EditorWindow::mode) {
            Some(EditorMode::Embedded) => {
                let room = ui.available_rect_before_wrap();
                ui.allocate_rect(room, Sense::hover());
                self.fx.embeds.insert(target, EmbedSpot { slot: slot_id, rect: room, layer });
            }
            Some(EditorMode::Floating) => {
                ui.label("This plug-in's editor is open in a separate window.");
                ui.horizontal(|ui| {
                    if ui.button("Bring to front").clicked() {
                        *action = Some(ChainAction::OpenSeparate(slot_id));
                    }
                    if ui.button("Show it here").clicked() {
                        *action = Some(ChainAction::ShowHere(slot_id));
                    }
                });
            }
            None => {
                if let Some(note) = self.fx.editor_notes.get(&slot_id) {
                    ui.add(egui::Label::new(egui::RichText::new(note).weak()).wrap());
                    ui.horizontal(|ui| {
                        if ui.button("Try again").clicked() {
                            self.fx.editor_notes.remove(&slot_id);
                        }
                        if ui.button("Show the parameters").clicked() {
                            *action = Some(ChainAction::SetView(FxView::Parameters));
                        }
                    });
                } else {
                    ui.weak("Opening the plug-in's editor...");
                    *action = Some(ChainAction::ShowHere(slot_id));
                    ui.ctx().request_repaint();
                }
            }
        }
    }

    /// A slider for each parameter; a double click puts it back to the value
    /// the plug-in starts with.
    fn show_fx_parameters(&mut self, ui: &mut egui::Ui, slot_id: u64, live: Option<Arc<LivePlugin>>) {
        let Some(handle) = live.and_then(|live| live.controller_handle()) else {
            ui.weak("This plug-in has no parameters to show.");
            return;
        };
        const KEEP: std::time::Duration = std::time::Duration::from_secs(2);
        let params = match self.fx.parameters.get(&slot_id) {
            Some((read, list)) if read.elapsed() < KEEP => list.clone(),
            _ => {
                let list = Arc::new(handle.parameters());
                self.fx.parameters.insert(slot_id, (std::time::Instant::now(), list.clone()));
                list
            }
        };
        if params.is_empty() {
            ui.weak("This plug-in has no parameters to show.");
            return;
        }
        // (parameter, value, a reset): applied after the list is drawn, so
        // the undo step can open before the first change.
        let mut changes: Vec<(u32, f64, bool)> = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt(("fx_params", slot_id))
            .auto_shrink([false, false])
            .show_rows(ui, ui.spacing().interact_size.y + 2.0, params.len(), |ui, range| {
                ui.spacing_mut().slider_width = (ui.available_width() - 260.0).clamp(80.0, 420.0);
                for param in &params[range] {
                    ui.horizontal(|ui| {
                        let name = ui
                            .add_sized([130.0, 18.0], egui::Label::new(&param.title).truncate().sense(Sense::click()))
                            .on_hover_text("Double-click to reset");
                        let mut value = handle.value(param.id) as f32;
                        let text = handle.text(param.id, value as f64);
                        let slider = egui::Slider::new(&mut value, 0.0..=1.0)
                            .show_value(false)
                            .custom_formatter(|_, _| String::new());
                        let response = ui.add_enabled(!param.read_only, slider);
                        ui.label(format!("{text} {}", param.units));
                        // A slider only senses drags, so its double click is
                        // read from the pointer.
                        let double_on_slider = response.contains_pointer()
                            && ui.input(|input| input.pointer.button_double_clicked(egui::PointerButton::Primary));
                        if !param.read_only && (double_on_slider || name.double_clicked()) {
                            changes.push((param.id, param.default, true));
                        } else if response.changed() {
                            changes.push((param.id, f64::from(value), false));
                        }
                    });
                }
            });
        for (id, value, reset) in changes {
            self.begin_param_edit(ui.ctx().viewport_id());
            handle.set(id, value);
            // A reset is a click, not a drag: it ends its own step.
            if reset {
                self.end_param_edit();
            }
        }
        self.finish_param_edit_when_released(ui.ctx());
    }

    /// Declares the Add FX browser.
    fn show_fx_browser(&mut self, ctx: &egui::Context) {
        let Some(browser) = &self.fx.browser else {
            return;
        };
        let name = self.fx_target_name(browser.target);
        let title = if browser.replace.is_some() {
            format!("Replace FX - {name}")
        } else {
            format!("Add FX to {name}")
        };
        self.floating_window(
            ctx,
            super::floating::WindowSpec::new("fx_browser_window", &title, Vec2::new(460.0, 480.0), Vec2::new(320.0, 240.0)),
            |app, ui| app.fx_browser_ui(ui),
            |app, _| app.fx.browser = None,
        );
    }

    fn fx_browser_ui(&mut self, ui: &mut egui::Ui) {
        let Some(browser) = &self.fx.browser else {
            return;
        };
        let target = browser.target;
        let replace = browser.replace;
        let mut filter = browser.filter.clone();
        let mut picked = browser.picked.clone();
        let mut commit: Option<ScannedPlugin> = None;
        let mut cancel = false;
        let mut rescan = false;
        let allow_instruments = target != FxTarget::Master;
        let accent = self.theme.accent;

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut filter)
                    .hint_text("Search plug-ins or developers")
                    .desired_width(240.0),
            );
            let scanning = self.plugin_scanner.is_scanning();
            if scanning {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
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

        if rescan {
            self.plugin_scanner.rescan();
        }
        if let Some(plugin) = commit {
            self.fx.browser = None;
            self.add_fx(target, &plugin, replace);
        } else if cancel {
            self.fx.browser = None;
        } else if let Some(browser) = &mut self.fx.browser {
            browser.filter = filter;
            browser.picked = picked;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_dragged_plug_in_lands_before_the_row_under_the_pointer() {
        let middles = [10.0, 30.0, 50.0];
        assert_eq!(insertion_index(&middles, 0.0), 0);
        assert_eq!(insertion_index(&middles, 20.0), 1);
        assert_eq!(insertion_index(&middles, 45.0), 2);
        assert_eq!(insertion_index(&middles, 90.0), 3);
    }

    #[test]
    fn reordering_moves_the_picked_plug_ins_together_keeping_their_order() {
        let chain = [1, 2, 3, 4, 5];
        assert_eq!(reordered(&chain, &[2], 4), vec![1, 3, 4, 2, 5]);
        assert_eq!(reordered(&chain, &[4], 1), vec![1, 4, 2, 3, 5]);
        assert_eq!(reordered(&chain, &[2, 4], 0), vec![2, 4, 1, 3, 5]);
        assert_eq!(reordered(&chain, &[1, 2], 5), vec![3, 4, 5, 1, 2]);
        assert_eq!(reordered(&chain, &[2, 3], 3), vec![1, 2, 3, 4, 5]);
        assert_eq!(reordered(&chain, &[3], 2), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_range_runs_from_one_row_to_another_in_either_direction() {
        let chain = [10, 20, 30, 40];
        assert_eq!(range_between(&chain, 20, 40), vec![20, 30, 40]);
        assert_eq!(range_between(&chain, 40, 20), vec![20, 30, 40]);
        assert_eq!(range_between(&chain, 30, 30), vec![30]);
        assert_eq!(range_between(&chain, 99, 30), vec![30]);
    }

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
