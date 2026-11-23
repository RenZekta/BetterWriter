//! Keeps the plug-ins of a project alive while it is open, and turns the
//! project's chains into the graph the audio thread runs.
//!
//! Each chain slot has an id; the host maps ids to running instances. An
//! instance is created the first time a chain that is switched on names it,
//! with the settings saved in the slot, and kept until the slot is gone.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::native_window::NativeWindow;
use super::vst3::{ControllerHandle, Editor, Gestures, Instance, Module};
use crate::core::{BwxProject, PluginFormat, SoundChainSlot};

/// What the plug-ins' own windows did since the last look.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowEdits {
    pub touched: bool,
    pub held: bool,
}

/// A plug-in the audio thread may run. The audio thread only ever
/// `try_lock`s, so the UI thread can read or change settings without
/// stalling the sound; it just misses a block when it holds the lock.
pub struct LivePlugin {
    pub id: u64,
    pub is_instrument: bool,
    pub instance: Mutex<Instance>,
    /// Taken once, when the plug-in starts, so the UI never needs the lock
    /// the audio thread runs the plug-in under. Every frame the UI held that
    /// lock was a block the audio thread had to skip, which is a click.
    controller: UiHandle,
    /// What the plug-in's own window has done to its parameters.
    pub gestures: Arc<Gestures>,
}

/// The controller half of a plug-in, which only the UI thread touches; the
/// audio thread never looks at it.
struct UiHandle(Option<ControllerHandle>);

// SAFETY: see `UiHandle`; the processing half is behind the mutex.
unsafe impl Send for UiHandle {}
unsafe impl Sync for UiHandle {}

/// What the audio thread runs: one chain per MIDI channel, then the master.
#[derive(Default)]
pub struct Graph {
    pub channels: [Vec<Arc<LivePlugin>>; 16],
    pub master: Vec<Arc<LivePlugin>>,
}

impl Graph {
    /// Equal keys mean the same plug-ins in the same places.
    pub fn key(&self) -> Vec<u64> {
        let mut key = Vec::new();
        for chain in self.channels.iter().chain(std::iter::once(&self.master)) {
            key.push(u64::MAX);
            key.extend(chain.iter().map(|plugin| plugin.id));
        }
        key
    }

    pub fn is_empty(&self) -> bool {
        self.channels.iter().all(Vec::is_empty) && self.master.is_empty()
    }
}

pub struct PluginHost {
    sample_rate: f64,
    modules: HashMap<String, Arc<Module>>,
    live: HashMap<u64, Arc<LivePlugin>>,
    failed: HashMap<u64, String>,
    /// Removed plug-ins wait here a moment so the audio thread has dropped
    /// its copy first and shutdown code never runs on it.
    retired: Vec<(Instant, Arc<LivePlugin>)>,
    /// Each running plug-in's settings as last written into a slot or read
    /// back: what a change made in the plug-in's own window started from.
    known_states: Mutex<HashMap<u64, Vec<u8>>>,
}

/// The ids of every slot of every chain of `project`.
pub fn slot_ids(project: &BwxProject) -> impl Iterator<Item = u64> + '_ {
    project
        .tracks
        .iter()
        .flat_map(|track| track.sound_chain.iter())
        .chain(project.master_chain.iter())
        .map(|slot| slot.id)
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            sample_rate: 44_100.0,
            modules: HashMap::new(),
            live: HashMap::new(),
            failed: HashMap::new(),
            retired: Vec::new(),
            known_states: Mutex::new(HashMap::new()),
        }
    }

    /// Sets the rate new plug-ins start at and moves the running ones to it.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        if (sample_rate - self.sample_rate).abs() < 0.5 {
            return;
        }
        self.sample_rate = sample_rate;
        for live in self.live.values() {
            if let Ok(mut instance) = live.instance.lock() {
                instance.set_sample_rate(sample_rate);
            }
        }
    }

    /// Why a slot's plug-in is not running, if it failed to start.
    pub fn error(&self, id: u64) -> Option<&str> {
        self.failed.get(&id).map(String::as_str)
    }

    /// Starts the slot's plug-in unless it already runs or already failed.
    pub fn ensure(&mut self, slot: &SoundChainSlot) -> Option<Arc<LivePlugin>> {
        if let Some(live) = self.live.get(&slot.id) {
            return Some(live.clone());
        }
        // A slot brought back (undo) finds its plug-in still waiting.
        if let Some(at) = self.retired.iter().position(|(_, live)| live.id == slot.id) {
            let (_, live) = self.retired.remove(at);
            self.live.insert(slot.id, live.clone());
            return Some(live);
        }
        if self.failed.contains_key(&slot.id) {
            return None;
        }
        match self.start(slot) {
            Ok(live) => {
                if let Ok(instance) = live.instance.lock() {
                    self.remember_state(slot.id, instance.save_state());
                }
                self.live.insert(slot.id, live.clone());
                Some(live)
            }
            Err(reason) => {
                self.failed.insert(slot.id, reason);
                None
            }
        }
    }

    /// Forgets a failure so the next `ensure` tries again.
    pub fn retry(&mut self, id: u64) {
        self.failed.remove(&id);
    }

    fn start(&mut self, slot: &SoundChainSlot) -> Result<Arc<LivePlugin>, String> {
        if slot.format != PluginFormat::Vst3 {
            return Err(format!(
                "{} plug-ins are listed but cannot be hosted yet; only VST3 can",
                slot.format.label()
            ));
        }
        let module = match self.modules.get(&slot.path) {
            Some(module) => module.clone(),
            None => {
                let module = Module::load(Path::new(&slot.path))?;
                self.modules.insert(slot.path.clone(), module.clone());
                module
            }
        };
        let instance = Instance::create(&module, &slot.class_id, self.sample_rate)?;
        instance.load_state(&slot.state);
        Ok(Arc::new(LivePlugin::new(slot.id, slot.is_instrument, instance)))
    }

    /// Drops every plug-in no slot of `project` names any more and that
    /// `history` (the slots the undo and redo steps name) does not keep, and
    /// ones retired long enough ago. A plug-in only waiting for an undo keeps
    /// running idle, so undoing its removal brings back the very instance,
    /// settings and all, rather than a new one.
    pub fn collect_garbage(&mut self, project: &BwxProject, history: &HashSet<u64>) {
        let wanted: HashSet<u64> = slot_ids(project).collect();
        let gone: Vec<u64> = self
            .live
            .keys()
            .filter(|id| !wanted.contains(id) && !history.contains(id))
            .copied()
            .collect();
        for id in gone {
            if let Some(live) = self.live.remove(&id) {
                self.retired.push((Instant::now(), live));
            }
        }
        self.failed.retain(|id, _| wanted.contains(id));
        self.retired
            .retain(|(since, _)| since.elapsed() < Duration::from_secs(3));
    }

    /// Writes each running plug-in's settings into its slot, for saving.
    pub fn capture_states(&self, project: &mut BwxProject) {
        let chains = project
            .tracks
            .iter_mut()
            .map(|track| &mut track.sound_chain)
            .chain(std::iter::once(&mut project.master_chain));
        for chain in chains {
            for slot in chain.iter_mut() {
                if let Some(live) = self.live.get(&slot.id)
                    && let Ok(instance) = live.instance.lock()
                {
                    let state = instance.save_state();
                    if !state.is_empty() {
                        self.remember_state(slot.id, state.clone());
                        slot.state = state;
                    }
                }
            }
        }
    }

    fn remember_state(&self, id: u64, state: Vec<u8>) {
        if let Ok(mut known) = self.known_states.lock() {
            known.insert(id, state);
        }
    }

    /// Whether a plug-in's own window changed something since the last call,
    /// and whether one of them is being held (a knob down) right now.
    pub fn poll_window_edits(&self) -> WindowEdits {
        let mut edits = WindowEdits::default();
        for live in self.live.values() {
            edits.touched |= live.gestures.take_touched();
            edits.held |= live.gestures.is_open();
        }
        edits
    }

    /// `project` with each plug-in's settings as they were before the
    /// changes made in its own window since they were last written down.
    pub fn with_known_states(&self, project: &BwxProject) -> BwxProject {
        let mut project = project.clone();
        let Ok(known) = self.known_states.lock() else {
            return project;
        };
        let chains = project
            .tracks
            .iter_mut()
            .map(|track| &mut track.sound_chain)
            .chain(std::iter::once(&mut project.master_chain));
        for chain in chains {
            for slot in chain.iter_mut() {
                if let Some(state) = known.get(&slot.id) {
                    slot.state = state.clone();
                }
            }
        }
        project
    }

    /// Puts each running plug-in's settings back to the ones its slot holds,
    /// for undo and redo. A plug-in that already has them is left alone.
    pub fn restore_states(&self, project: &BwxProject) {
        let chains = project
            .tracks
            .iter()
            .map(|track| track.sound_chain.as_slice())
            .chain(std::iter::once(project.master_chain.as_slice()));
        for chain in chains {
            for slot in chain {
                if slot.state.is_empty() {
                    continue;
                }
                if let Some(live) = self.live.get(&slot.id)
                    && let Ok(instance) = live.instance.lock()
                    && instance.save_state() != slot.state
                {
                    instance.load_state(&slot.state);
                }
                self.remember_state(slot.id, slot.state.clone());
            }
        }
        // Settings loaded from here are not the user's changes.
        for live in self.live.values() {
            live.gestures.take_touched();
        }
    }

    /// The graph the project's chains describe. Switched-off chains and
    /// bypassed slots are left out; so are plug-ins that failed to start.
    pub fn build_graph(&mut self, project: &BwxProject) -> Graph {
        let mut graph = Graph::default();
        for track in &project.tracks {
            let channel = usize::from(track.channel.min(15));
            if track.fx_bypassed || !graph.channels[channel].is_empty() {
                continue;
            }
            let mut chain = Vec::new();
            for slot in track.sound_chain.iter().filter(|slot| !slot.bypassed) {
                if let Some(live) = self.ensure(slot) {
                    chain.push(live);
                }
            }
            graph.channels[channel] = chain;
        }
        if !project.master_fx_bypassed {
            for slot in project.master_chain.iter().filter(|slot| !slot.bypassed) {
                if let Some(live) = self.ensure(slot) {
                    graph.master.push(live);
                }
            }
        }
        graph
    }
}

impl LivePlugin {
    pub fn new(id: u64, is_instrument: bool, instance: Instance) -> Self {
        let controller = UiHandle(instance.controller_handle());
        let gestures = instance.gestures();
        Self {
            id,
            gestures,
            is_instrument,
            instance: Mutex::new(instance),
            controller,
        }
    }

    /// The handle the UI thread shows parameters through.
    pub fn controller_handle(&self) -> Option<ControllerHandle> {
        self.controller.0.clone()
    }
}

/// Where a plug-in's editor is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorMode {
    /// A window of its own, with a title bar.
    Floating,
    /// A child window laid over a part of the program's own window.
    Embedded,
}

/// A plug-in's own editor, open in a native window.
pub struct EditorWindow {
    // The view comes off before its window goes.
    editor: Editor,
    window: NativeWindow,
    /// Keeps the plug-in running for as long as its view exists, so the
    /// view never outlives the instance it belongs to. Last, so it goes last.
    _plugin: Arc<LivePlugin>,
    mode: EditorMode,
    /// The size the plug-in wants, in pixels.
    size: (i32, i32),
    can_resize: bool,
    /// The size last given to the view, so it is only told about changes.
    last_view_size: Option<(i32, i32)>,
}

fn editors_need_windows() -> Result<(), String> {
    if cfg!(windows) {
        Ok(())
    } else {
        Err(
            "Plug-in editor windows are only available on Windows for now; the parameters work everywhere"
                .to_owned(),
        )
    }
}

impl EditorWindow {
    /// Opens the editor in a window of its own, owned by `owner` (the
    /// program's window, which it then stays above; null for none).
    pub fn open(title: &str, plugin: &Arc<LivePlugin>, owner: *mut std::ffi::c_void) -> Result<EditorWindow, String> {
        editors_need_windows()?;
        let handle = plugin.controller_handle().ok_or("the plug-in has no controller")?;
        let mut editor = Editor::new(handle.controller())?;
        let (width, height) = editor.size();
        let can_resize = editor.can_resize();
        let window = NativeWindow::create(title, width, height, can_resize, owner)?;
        editor.attach(window.handle())?;
        window.focus();
        window.redraw();
        Ok(EditorWindow {
            editor,
            window,
            _plugin: plugin.clone(),
            mode: EditorMode::Floating,
            size: (width, height),
            can_resize,
            last_view_size: Some((width, height)),
        })
    }

    /// Opens the editor as a child of the program's main window `parent`. It
    /// stays hidden until `place` puts it somewhere.
    pub fn open_embedded(parent: *mut std::ffi::c_void, plugin: &Arc<LivePlugin>) -> Result<EditorWindow, String> {
        editors_need_windows()?;
        let handle = plugin.controller_handle().ok_or("the plug-in has no controller")?;
        let mut editor = Editor::new(handle.controller())?;
        let (width, height) = editor.size();
        let can_resize = editor.can_resize();
        let window = NativeWindow::create_child(parent, width, height)?;
        editor.attach(window.handle())?;
        Ok(EditorWindow {
            editor,
            window,
            _plugin: plugin.clone(),
            mode: EditorMode::Embedded,
            size: (width, height),
            can_resize,
            last_view_size: Some((width, height)),
        })
    }

    pub fn mode(&self) -> EditorMode {
        self.mode
    }

    /// The size the plug-in's editor wants, in pixels.
    pub fn wanted_size(&self) -> (i32, i32) {
        self.size
    }

    /// False once the user closed the window.
    pub fn is_open(&self) -> bool {
        self.window.is_open()
    }

    /// Follows a size the plug-in asked for, and, in a window the user can
    /// resize, tells the editor the window's new size.
    pub fn update(&mut self) {
        if let Some((width, height)) = self.editor.take_resize_request() {
            self.size = (width, height);
            self.last_view_size = Some((width, height));
            if self.mode == EditorMode::Floating {
                self.window.resize(width, height);
            }
        }
        if self.mode == EditorMode::Floating
            && self.window.is_resizable()
            && let Some(client) = self.window.client_size()
            && client.0 > 0
            && client.1 > 0
            && Some(client) != self.last_view_size
        {
            self.editor.resize(client.0, client.1);
            self.last_view_size = Some(client);
        }
    }

    /// Puts an embedded editor over a rectangle of the main window (pixels),
    /// or hides it. A hidden editor is left at the size it had.
    pub fn place(&mut self, x: i32, y: i32, width: i32, height: i32, visible: bool) {
        if self.mode != EditorMode::Embedded {
            return;
        }
        if !visible {
            self.window.hide();
            return;
        }
        // An editor that cannot be resized keeps its own size, cut off where
        // the room given to it ends.
        let (width, height) = if self.can_resize {
            (width, height)
        } else {
            (self.size.0.min(width), self.size.1.min(height))
        };
        if self.can_resize && self.last_view_size != Some((width, height)) {
            self.editor.resize(width, height);
            self.last_view_size = Some((width, height));
        }
        self.window.place(x, y, width, height, visible);
    }

    /// True while the keyboard goes to the editor.
    pub fn has_keyboard(&self) -> bool {
        self.window.has_keyboard()
    }

    pub fn can_resize(&self) -> bool {
        self.can_resize
    }

    pub fn focus(&self) {
        self.window.focus();
    }
}
