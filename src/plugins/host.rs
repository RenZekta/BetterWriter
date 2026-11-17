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
use super::vst3::{ControllerHandle, Editor, Instance, Module};
use crate::core::{BwxProject, PluginFormat, SoundChainSlot};

/// A plug-in the audio thread may run. The audio thread only ever
/// `try_lock`s, so the UI thread can read or change settings without
/// stalling the sound; it just misses a block when it holds the lock.
pub struct LivePlugin {
    pub id: u64,
    pub is_instrument: bool,
    pub instance: Mutex<Instance>,
}

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
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
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
        if self.failed.contains_key(&slot.id) {
            return None;
        }
        match self.start(slot) {
            Ok(live) => {
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
        Ok(Arc::new(LivePlugin {
            id: slot.id,
            is_instrument: slot.is_instrument,
            instance: Mutex::new(instance),
        }))
    }

    /// Drops every plug-in no slot of `project` names any more, and ones
    /// retired long enough ago.
    pub fn collect_garbage(&mut self, project: &BwxProject) {
        let wanted: HashSet<u64> = project
            .tracks
            .iter()
            .flat_map(|track| track.sound_chain.iter())
            .chain(project.master_chain.iter())
            .map(|slot| slot.id)
            .collect();
        let gone: Vec<u64> = self.live.keys().filter(|id| !wanted.contains(id)).copied().collect();
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
                        slot.state = state;
                    }
                }
            }
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
    /// The handle the UI thread shows parameters through.
    pub fn controller_handle(&self) -> Option<ControllerHandle> {
        self.instance.lock().ok().and_then(|instance| instance.controller_handle())
    }
}

/// A plug-in's own editor, open in a window of its own.
pub struct EditorWindow {
    // The view comes off before its window goes.
    editor: Editor,
    window: NativeWindow,
}

impl EditorWindow {
    pub fn open(title: &str, plugin: &LivePlugin) -> Result<EditorWindow, String> {
        if !cfg!(windows) {
            return Err(
                "Plug-in editor windows are only available on Windows for now; the parameters below work everywhere"
                    .to_owned(),
            );
        }
        let handle = plugin.controller_handle().ok_or("the plug-in has no controller")?;
        let mut editor = Editor::new(handle.controller())?;
        let (width, height) = editor.size();
        let window = NativeWindow::create(title, width, height)?;
        editor.attach(window.handle())?;
        window.focus();
        Ok(EditorWindow { editor, window })
    }

    /// False once the user closed the window.
    pub fn is_open(&self) -> bool {
        self.window.is_open()
    }

    /// Follows a size the plug-in asked for.
    pub fn update(&mut self) {
        if let Some((width, height)) = self.editor.take_resize_request() {
            self.window.resize(width, height);
        }
    }

    pub fn focus(&self) {
        self.window.focus();
    }
}
