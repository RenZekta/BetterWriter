//! A small VST3 host: loads a bundle, lists its classes, runs a plug-in on
//! blocks of audio and MIDI notes, saves and restores its settings, and
//! hands out its editor view.
//!
//! The VST3 interfaces are raw COM, so most of this file is `unsafe`. Every
//! call into plug-in code happens behind `Result`s and `Option`s so a plug-in
//! that refuses a step is dropped from the chain instead of taking the
//! program down.

#![allow(non_snake_case)]

use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering},
};

use vst3::{Class, ComPtr, ComRef, ComWrapper, Interface, Steinberg::Vst::*, Steinberg::*};

use super::scan::{ProbedClass, PluginKind, format_probe_line, kind_from_sub_categories};

type Factory = ComPtr<IPluginFactory>;

fn chars_to_string(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn copy_wide(text: &str, out: &mut [TChar]) {
    let mut len = 0;
    for (unit, slot) in text.encode_utf16().zip(out.iter_mut()) {
        *slot = unit as TChar;
        len += 1;
    }
    if len < out.len() {
        out[len] = 0;
    } else if let Some(last) = out.last_mut() {
        *last = 0;
    }
}

fn class_id_to_hex(id: &TUID) -> String {
    id.iter().map(|byte| format!("{:02x}", *byte as u8)).collect()
}

fn class_id_from_hex(hex: &str) -> Option<TUID> {
    if hex.len() != 32 {
        return None;
    }
    let mut id: TUID = [0; 16];
    for (index, slot) in id.iter_mut().enumerate() {
        *slot = u8::from_str_radix(hex.get(index * 2..index * 2 + 2)?, 16).ok()? as c_char;
    }
    Some(id)
}

fn ok(result: tresult) -> bool {
    result == kResultOk
}

/// One audio class a bundle's factory offers.
#[derive(Clone, Debug)]
pub struct ClassInfo {
    pub class_id: String,
    pub name: String,
    pub vendor: String,
    pub sub_categories: String,
}

/// A loaded bundle. Dropping it releases the factory, runs the module's exit
/// function and unloads the library, in that order.
pub struct Module {
    factory: Option<Factory>,
    exit: Option<unsafe extern "system" fn() -> bool>,
    library: Option<LibraryHandle>,
}

// The factory is thread-safe by specification and the module is only ever
// shared behind an `Arc`.
unsafe impl Send for Module {}
unsafe impl Sync for Module {}

#[cfg(unix)]
type LibraryHandle = libloading::os::unix::Library;
#[cfg(windows)]
type LibraryHandle = libloading::os::windows::Library;

/// Where the loadable file lives inside a bundle (or the file itself for the
/// old single-file layout on Windows).
fn binary_path(bundle: &Path) -> Result<PathBuf, String> {
    if bundle.is_file() {
        return Ok(bundle.to_path_buf());
    }
    let stem = bundle
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .ok_or("the plug-in has no name")?;
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" };
    let candidates: Vec<PathBuf> = if cfg!(target_os = "windows") {
        let win = if cfg!(target_arch = "aarch64") { "arm64-win" } else { "x86_64-win" };
        vec![
            bundle.join("Contents").join(win).join(format!("{stem}.vst3")),
            bundle.join("Contents").join("x86-win").join(format!("{stem}.vst3")),
        ]
    } else if cfg!(target_os = "macos") {
        vec![bundle.join("Contents/MacOS").join(&stem)]
    } else {
        vec![
            bundle
                .join("Contents")
                .join(format!("{arch}-linux"))
                .join(format!("{stem}.so")),
        ]
    };
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("{}: no {arch} binary in the bundle", bundle.display()))
}

impl Module {
    pub fn load(bundle: &Path) -> Result<Arc<Module>, String> {
        if cfg!(target_os = "macos") {
            return Err("loading plug-ins is not implemented on macOS yet".to_owned());
        }
        let binary = binary_path(bundle)?;
        // SAFETY: loading a plug-in runs its initialisation code, which is
        // the point; a failure to load is reported, not unwound.
        unsafe {
            #[cfg(unix)]
            let library = LibraryHandle::open(Some(&binary), libloading::os::unix::RTLD_NOW)
                .map_err(|err| format!("{}: {err}", binary.display()))?;
            #[cfg(windows)]
            let library = LibraryHandle::load_with_flags(
                &binary,
                libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH,
            )
            .map_err(|err| format!("{}: {err}", binary.display()))?;

            #[cfg(windows)]
            let (library, exit) = {
                if let Ok(entry) = library.get::<unsafe extern "system" fn() -> bool>(b"InitDll\0") {
                    entry();
                }
                let exit = library
                    .get::<unsafe extern "system" fn() -> bool>(b"ExitDll\0")
                    .ok()
                    .map(|symbol| *symbol);
                (library, exit)
            };
            #[cfg(target_os = "linux")]
            let (library, exit) = {
                let entry = library
                    .get::<unsafe extern "system" fn(*mut c_void) -> bool>(b"ModuleEntry\0")
                    .ok()
                    .map(|symbol| *symbol);
                let exit = library
                    .get::<unsafe extern "system" fn() -> bool>(b"ModuleExit\0")
                    .ok()
                    .map(|symbol| *symbol);
                // ModuleEntry is handed the loader's handle for the library.
                let raw = library.into_raw();
                if let Some(entry) = entry {
                    entry(raw);
                }
                (LibraryHandle::from_raw(raw), exit)
            };
            // Loading is refused on macOS above, so this only keeps the
            // other platforms' shape (and lets the code build there).
            #[cfg(not(any(windows, target_os = "linux")))]
            let exit: Option<unsafe extern "system" fn() -> bool> = None;
            let get_factory = library
                .get::<unsafe extern "system" fn() -> *mut IPluginFactory>(b"GetPluginFactory\0")
                .map_err(|err| format!("{}: {err}", binary.display()))?;
            let factory = ComPtr::from_raw(get_factory())
                .ok_or_else(|| format!("{}: no plug-in factory", binary.display()))?;
            Ok(Arc::new(Module {
                factory: Some(factory),
                exit,
                library: Some(library),
            }))
        }
    }

    fn factory(&self) -> &Factory {
        self.factory.as_ref().expect("factory lives as long as the module")
    }


    /// The vendor the factory itself names.
    pub fn factory_vendor(&self) -> String {
        let mut info: PFactoryInfo = unsafe { std::mem::zeroed() };
        // SAFETY: `info` is a valid, zeroed out-parameter.
        if unsafe { self.factory().getFactoryInfo(&mut info) } == kResultOk {
            chars_to_string(&info.vendor)
        } else {
            String::new()
        }
    }

    /// The audio-processing classes of the bundle.
    pub fn classes(&self) -> Vec<ClassInfo> {
        let factory = self.factory();
        let factory_vendor = self.factory_vendor();
        let factory2 = factory.cast::<IPluginFactory2>();
        let count = unsafe { factory.countClasses() };
        let mut classes = Vec::new();
        for index in 0..count.max(0) {
            let (id, category, name, vendor, sub_categories) = if let Some(factory2) = &factory2 {
                let mut info: PClassInfo2 = unsafe { std::mem::zeroed() };
                if !ok(unsafe { factory2.getClassInfo2(index, &mut info) }) {
                    continue;
                }
                (
                    info.cid,
                    chars_to_string(&info.category),
                    chars_to_string(&info.name),
                    chars_to_string(&info.vendor),
                    chars_to_string(&info.subCategories),
                )
            } else {
                let mut info: PClassInfo = unsafe { std::mem::zeroed() };
                if !ok(unsafe { factory.getClassInfo(index, &mut info) }) {
                    continue;
                }
                (
                    info.cid,
                    chars_to_string(&info.category),
                    chars_to_string(&info.name),
                    String::new(),
                    String::new(),
                )
            };
            if category != "Audio Module Class" {
                continue;
            }
            classes.push(ClassInfo {
                class_id: class_id_to_hex(&id),
                name,
                vendor: if vendor.is_empty() { factory_vendor.clone() } else { vendor },
                sub_categories,
            });
        }
        classes
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        self.factory = None;
        if let Some(exit) = self.exit.take() {
            // SAFETY: the exit function belongs to the library still loaded.
            unsafe {
                exit();
            }
        }
        self.library = None;
    }
}

// ---------------------------------------------------------------- probing

/// What the `--probe-vst3 <bundle>` child process runs: loads the bundle and
/// prints one line per audio class. Returns the process exit code.
pub fn probe_main(path: &Path) -> i32 {
    let module = match Module::load(path) {
        Ok(module) => module,
        Err(_) => return 2,
    };
    let classes = module.classes();
    if classes.is_empty() {
        return 3;
    }
    for class in classes {
        let probed = ProbedClass {
            class_id: class.class_id.clone(),
            name: class.name.clone(),
            vendor: class.vendor.clone(),
            kind: kind_from_sub_categories(&class.sub_categories),
        };
        println!("{}", format_probe_line(&probed, &class.sub_categories));
    }
    // The module is leaked on purpose: unloading plug-in code in a child that
    // is about to exit only gives it a chance to crash.
    std::mem::forget(module);
    0
}

#[allow(dead_code)]
pub fn kind_of(class: &ClassInfo) -> PluginKind {
    kind_from_sub_categories(&class.sub_categories)
}

// ------------------------------------------------------------ host objects

/// What the plug-in is told about its host.
struct HostContext;

impl Class for HostContext {
    type Interfaces = (IHostApplication,);
}

impl IHostApplicationTrait for HostContext {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        copy_wide("BetterWriter", unsafe { &mut *name });
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        _cid: *mut TUID,
        _iid: *mut TUID,
        _obj: *mut *mut c_void,
    ) -> tresult {
        kNotImplemented
    }
}

/// Parameter edits the plug-in's own window makes, waiting to reach the
/// audio processor.
type EditQueue = Arc<Mutex<Vec<(u32, f64)>>>;

/// What the plug-in's own window has done to its parameters, for the host's
/// edit history to read from the UI thread.
#[derive(Default)]
pub struct Gestures {
    /// Gestures begun and not yet ended (a knob held down).
    open: std::sync::atomic::AtomicI32,
    /// The plug-in changed a parameter since the last `take_touched`.
    touched: std::sync::atomic::AtomicBool,
}

impl Gestures {
    pub fn is_open(&self) -> bool {
        self.open.load(std::sync::atomic::Ordering::Relaxed) > 0
    }

    /// Whether the plug-in changed anything since the last call.
    pub fn take_touched(&self) -> bool {
        self.touched.swap(false, std::sync::atomic::Ordering::Relaxed)
    }
}

struct ComponentHandler {
    edits: EditQueue,
    gestures: Arc<Gestures>,
}

impl Class for ComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for ComponentHandler {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        use std::sync::atomic::Ordering::Relaxed;
        self.gestures.open.fetch_add(1, Relaxed);
        self.gestures.touched.store(true, Relaxed);
        kResultOk
    }

    unsafe fn performEdit(&self, id: ParamID, value_normalized: ParamValue) -> tresult {
        self.gestures.touched.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut edits) = self.edits.lock() {
            edits.push((id, value_normalized));
        }
        kResultOk
    }

    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        use std::sync::atomic::Ordering::Relaxed;
        let _ = self.gestures.open.try_update(Relaxed, Relaxed, |open| Some((open - 1).max(0)));
        kResultOk
    }

    unsafe fn restartComponent(&self, _flags: int32) -> tresult {
        kResultOk
    }
}

/// A note event for an instrument plug-in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteEvent {
    /// Sample position inside the block.
    pub offset: u32,
    pub on: bool,
    pub channel: u8,
    pub key: u8,
    /// 0.0 to 1.0.
    pub velocity: f32,
}

/// Reused for every block, so the audio thread never allocates for events.
struct EventList {
    events: Mutex<Vec<Event>>,
}

impl Class for EventList {
    type Interfaces = (IEventList,);
}

impl IEventListTrait for EventList {
    unsafe fn getEventCount(&self) -> int32 {
        self.events.lock().map_or(0, |events| events.len() as int32)
    }

    unsafe fn getEvent(&self, index: int32, event: *mut Event) -> tresult {
        let Ok(events) = self.events.lock() else {
            return kInvalidArgument;
        };
        match events.get(index as usize) {
            Some(found) => {
                unsafe { *event = *found };
                kResultOk
            }
            None => kInvalidArgument,
        }
    }

    unsafe fn addEvent(&self, _event: *mut Event) -> tresult {
        kResultOk
    }
}

fn note_event(note: &NoteEvent) -> Event {
    let mut event: Event = unsafe { std::mem::zeroed() };
    event.busIndex = 0;
    event.sampleOffset = note.offset as int32;
    event.flags = Event_::EventFlags_::kIsLive as u16;
    if note.on {
        event.r#type = Event_::EventTypes_::kNoteOnEvent as u16;
        event.__field0.noteOn = NoteOnEvent {
            channel: note.channel as i16,
            pitch: note.key as i16,
            tuning: 0.0,
            velocity: note.velocity,
            length: 0,
            noteId: note.key as int32,
        };
    } else {
        event.r#type = Event_::EventTypes_::kNoteOffEvent as u16;
        event.__field0.noteOff = NoteOffEvent {
            channel: note.channel as i16,
            pitch: note.key as i16,
            velocity: note.velocity,
            noteId: note.key as int32,
            tuning: 0.0,
        };
    }
    event
}

/// One changed parameter. Made once, refilled for every block.
struct ParamQueue {
    id: AtomicU32,
    value: AtomicU64,
}

impl ParamQueue {
    fn set(&self, id: ParamID, value: ParamValue) {
        self.id.store(id, Ordering::Relaxed);
        self.value.store(value.to_bits(), Ordering::Relaxed);
    }

    fn value(&self) -> ParamValue {
        f64::from_bits(self.value.load(Ordering::Relaxed))
    }
}

impl Class for ParamQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for ParamQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id.load(Ordering::Relaxed)
    }

    unsafe fn getPointCount(&self) -> int32 {
        1
    }

    unsafe fn getPoint(
        &self,
        index: int32,
        sample_offset: *mut int32,
        value: *mut ParamValue,
    ) -> tresult {
        if index != 0 {
            return kInvalidArgument;
        }
        unsafe {
            *sample_offset = 0;
            *value = self.value();
        }
        kResultOk
    }

    unsafe fn addPoint(
        &self,
        _sample_offset: int32,
        _value: ParamValue,
        _index: *mut int32,
    ) -> tresult {
        kResultFalse
    }
}

/// How many different parameters one block can carry; more wait for the next.
const MAX_PARAM_CHANGES: usize = 64;

struct ParamChanges {
    queues: Vec<ComWrapper<ParamQueue>>,
    /// The queues in use this block.
    count: AtomicUsize,
}

impl ParamChanges {
    fn new() -> Self {
        Self {
            queues: (0..MAX_PARAM_CHANGES)
                .map(|_| {
                    ComWrapper::new(ParamQueue {
                        id: AtomicU32::new(0),
                        value: AtomicU64::new(0),
                    })
                })
                .collect(),
            count: AtomicUsize::new(0),
        }
    }
}

impl Class for ParamChanges {
    type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for ParamChanges {
    unsafe fn getParameterCount(&self) -> int32 {
        self.count.load(Ordering::Relaxed) as int32
    }

    unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
        if index < 0 || index as usize >= self.count.load(Ordering::Relaxed) {
            return std::ptr::null_mut();
        }
        self.queues
            .get(index as usize)
            .and_then(|queue| queue.as_com_ref::<IParamValueQueue>())
            .map_or(std::ptr::null_mut(), |queue| queue.as_ptr())
    }

    unsafe fn addParameterData(&self, _id: *const ParamID, _index: *mut int32) -> *mut IParamValueQueue {
        std::ptr::null_mut()
    }
}

/// An in-memory `IBStream`, for saving and restoring plug-in settings.
struct MemoryStream {
    state: Mutex<(Vec<u8>, usize)>,
}

impl MemoryStream {
    fn new(data: Vec<u8>) -> ComWrapper<MemoryStream> {
        ComWrapper::new(MemoryStream { state: Mutex::new((data, 0)) })
    }
}

impl Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(&self, buffer: *mut c_void, num_bytes: int32, num_read: *mut int32) -> tresult {
        let Ok(mut state) = self.state.lock() else {
            return kResultFalse;
        };
        let (data, position) = &mut *state;
        let available = data.len().saturating_sub(*position);
        let count = (num_bytes.max(0) as usize).min(available);
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr().add(*position), buffer as *mut u8, count);
            if !num_read.is_null() {
                *num_read = count as int32;
            }
        }
        *position += count;
        kResultOk
    }

    unsafe fn write(&self, buffer: *mut c_void, num_bytes: int32, num_written: *mut int32) -> tresult {
        let Ok(mut state) = self.state.lock() else {
            return kResultFalse;
        };
        let (data, position) = &mut *state;
        let count = num_bytes.max(0) as usize;
        if data.len() < *position + count {
            data.resize(*position + count, 0);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(buffer as *const u8, data.as_mut_ptr().add(*position), count);
            if !num_written.is_null() {
                *num_written = count as int32;
            }
        }
        *position += count;
        kResultOk
    }

    unsafe fn seek(&self, pos: int64, mode: int32, result: *mut int64) -> tresult {
        let Ok(mut state) = self.state.lock() else {
            return kResultFalse;
        };
        let (data, position) = &mut *state;
        let target = match mode {
            0 => pos,                           // set
            1 => *position as int64 + pos,      // current
            2 => data.len() as int64 + pos,     // end
            _ => return kInvalidArgument,
        };
        if target < 0 {
            return kInvalidArgument;
        }
        *position = target as usize;
        if !result.is_null() {
            unsafe { *result = target };
        }
        kResultOk
    }

    unsafe fn tell(&self, pos: *mut int64) -> tresult {
        let Ok(state) = self.state.lock() else {
            return kResultFalse;
        };
        if !pos.is_null() {
            unsafe { *pos = state.1 as int64 };
        }
        kResultOk
    }
}

// --------------------------------------------------------------- instances

/// The size of a processing block the plug-in is told to expect at most.
pub const MAX_BLOCK: usize = 4096;

/// A running plug-in: its processor, its controller and the host objects it
/// holds on to.
pub struct Instance {
    component: ComPtr<IComponent>,
    processor: ComPtr<IAudioProcessor>,
    controller: Option<ComPtr<IEditController>>,
    /// The controller is its own object, created and initialised here.
    separate_controller: bool,
    _host: ComWrapper<HostContext>,
    _handler: ComWrapper<ComponentHandler>,
    edits: EditQueue,
    gestures: Arc<Gestures>,
    /// Channels of the main audio input; 0 for an instrument.
    pub inputs: usize,
    pub outputs: usize,
    scratch_in: [Vec<f32>; 2],
    scratch_out: [Vec<f32>; 2],
    /// Host objects handed to every `process` call, reused so the audio
    /// thread does not allocate.
    events: ComWrapper<EventList>,
    changes: ComWrapper<ParamChanges>,
    /// Parameter changes that did not fit the block they were taken in.
    pending: Vec<(u32, f64)>,
    sample_rate: f64,
    running: bool,
    /// Declared last so it drops last: the library must stay loaded until
    /// every object the plug-in handed out has been released.
    _module: Arc<Module>,
}

// The processing half is only touched under a mutex by one thread at a time;
// the controller half is only touched on the UI thread.
unsafe impl Send for Instance {}

impl Instance {
    pub fn create(
        module: &Arc<Module>,
        class_id_hex: &str,
        sample_rate: f64,
    ) -> Result<Instance, String> {
        let classes = module.classes();
        let chosen = if class_id_hex.is_empty() {
            classes.first()
        } else {
            classes.iter().find(|class| class.class_id == class_id_hex)
        }
        .ok_or("the plug-in has no audio class")?;
        let class_id = class_id_from_hex(&chosen.class_id).ok_or("bad class id")?;
        let factory = module.factory();

        // SAFETY: plain COM calls on pointers the factory gave out; each
        // failure returns an error and releases what was made so far.
        unsafe {
            let mut raw: *mut c_void = std::ptr::null_mut();
            let result = factory.createInstance(
                class_id.as_ptr(),
                IComponent::IID.as_ptr() as *const c_char,
                &mut raw,
            );
            if !ok(result) || raw.is_null() {
                return Err(format!("{}: could not create the plug-in", chosen.name));
            }
            let component = ComPtr::<IComponent>::from_raw(raw as *mut IComponent)
                .ok_or("could not create the plug-in")?;

            let host = ComWrapper::new(HostContext);
            let host_ptr = host
                .as_com_ref::<IHostApplication>()
                .ok_or("host context")?
                .as_ptr() as *mut FUnknown;
            if !ok(component.initialize(host_ptr)) {
                return Err(format!("{}: initialisation failed", chosen.name));
            }
            let processor = component
                .cast::<IAudioProcessor>()
                .ok_or_else(|| format!("{}: not an audio processor", chosen.name))?;

            // The controller: the component itself, or a class it names.
            let mut separate_controller = false;
            let mut controller = component.cast::<IEditController>();
            if controller.is_none() {
                let mut controller_id: TUID = [0; 16];
                if ok(component.getControllerClassId(&mut controller_id)) {
                    let mut raw: *mut c_void = std::ptr::null_mut();
                    let result = factory.createInstance(
                        controller_id.as_ptr(),
                        IEditController::IID.as_ptr() as *const c_char,
                        &mut raw,
                    );
                    if ok(result)
                        && let Some(created) = ComPtr::<IEditController>::from_raw(raw as *mut IEditController)
                    {
                        if ok(created.initialize(host_ptr)) {
                            separate_controller = true;
                            controller = Some(created);
                        }
                    }
                }
            }
            let edits: EditQueue = Arc::new(Mutex::new(Vec::new()));
            let gestures = Arc::new(Gestures::default());
            let handler = ComWrapper::new(ComponentHandler { edits: edits.clone(), gestures: gestures.clone() });
            if let Some(controller) = &controller {
                if let Some(handler_ref) = handler.as_com_ref::<IComponentHandler>() {
                    controller.setComponentHandler(handler_ref.as_ptr());
                }
                if separate_controller
                    && let (Some(a), Some(b)) = (
                        component.cast::<IConnectionPoint>(),
                        controller.cast::<IConnectionPoint>(),
                    )
                {
                    a.connect(b.as_ptr());
                    b.connect(a.as_ptr());
                }
            }

            // Buses: the first audio input and output, and a note input.
            let input_buses = component.getBusCount(MediaTypes_::kAudio as MediaType, BusDirections_::kInput as BusDirection);
            let output_buses = component.getBusCount(MediaTypes_::kAudio as MediaType, BusDirections_::kOutput as BusDirection);
            let event_buses = component.getBusCount(MediaTypes_::kEvent as MediaType, BusDirections_::kInput as BusDirection);
            if output_buses < 1 {
                return Err(format!("{}: has no audio output", chosen.name));
            }
            let mut stereo = SpeakerArr::kStereo;
            let mut ins = [stereo];
            let mut outs = [stereo];
            let in_count = if input_buses > 0 { 1 } else { 0 };
            processor.setBusArrangements(
                if in_count > 0 { ins.as_mut_ptr() } else { std::ptr::null_mut() },
                in_count,
                outs.as_mut_ptr(),
                1,
            );
            stereo = SpeakerArr::kStereo;
            let _ = stereo;
            let channels_of = |dir: BusDirection| -> usize {
                let mut arrangement: SpeakerArrangement = 0;
                if ok(processor.getBusArrangement(dir, 0, &mut arrangement)) {
                    (arrangement.count_ones() as usize).clamp(1, 2)
                } else {
                    2
                }
            };
            let inputs = if in_count > 0 { channels_of(BusDirections_::kInput as BusDirection) } else { 0 };
            let outputs = channels_of(BusDirections_::kOutput as BusDirection);
            for index in 0..input_buses.min(1) {
                component.activateBus(MediaTypes_::kAudio as MediaType, BusDirections_::kInput as BusDirection, index, 1);
            }
            component.activateBus(MediaTypes_::kAudio as MediaType, BusDirections_::kOutput as BusDirection, 0, 1);
            for index in 0..event_buses.min(1) {
                component.activateBus(MediaTypes_::kEvent as MediaType, BusDirections_::kInput as BusDirection, index, 1);
            }

            let mut setup = ProcessSetup {
                processMode: ProcessModes_::kRealtime as int32,
                symbolicSampleSize: SymbolicSampleSizes_::kSample32 as int32,
                maxSamplesPerBlock: MAX_BLOCK as int32,
                sampleRate: sample_rate,
            };
            if processor.canProcessSampleSize(SymbolicSampleSizes_::kSample32 as int32) != kResultOk {
                return Err(format!("{}: cannot process 32-bit audio", chosen.name));
            }
            if !ok(processor.setupProcessing(&mut setup)) {
                return Err(format!("{}: refused the audio setup", chosen.name));
            }
            if !ok(component.setActive(1)) {
                return Err(format!("{}: could not be activated", chosen.name));
            }
            processor.setProcessing(1);

            Ok(Instance {
                _module: module.clone(),
                component,
                processor,
                controller,
                separate_controller,
                _host: host,
                _handler: handler,
                edits,
                gestures,
                inputs,
                outputs,
                scratch_in: [vec![0.0; MAX_BLOCK], vec![0.0; MAX_BLOCK]],
                scratch_out: [vec![0.0; MAX_BLOCK], vec![0.0; MAX_BLOCK]],
                events: ComWrapper::new(EventList {
                    events: Mutex::new(Vec::with_capacity(512)),
                }),
                changes: ComWrapper::new(ParamChanges::new()),
                pending: Vec::with_capacity(256),
                sample_rate,
                running: true,
            })
        }
    }

    /// Runs one block. `left`/`right` hold the audio coming in (ignored by
    /// instruments) and are replaced with what comes out.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32], notes: &[NoteEvent]) {
        let frames = left.len().min(right.len()).min(MAX_BLOCK);
        if frames == 0 || !self.running {
            return;
        }
        // A plug-in that reports silence need not write its output, so the
        // buffers start clean and are never left holding an earlier block.
        self.scratch_out[0][..frames].fill(0.0);
        self.scratch_out[1][..frames].fill(0.0);
        self.scratch_in[0][..frames].copy_from_slice(&left[..frames]);
        self.scratch_in[1][..frames].copy_from_slice(&right[..frames]);

        let mut in_ptrs = [self.scratch_in[0].as_mut_ptr(), self.scratch_in[1].as_mut_ptr()];
        let mut out_ptrs = [self.scratch_out[0].as_mut_ptr(), self.scratch_out[1].as_mut_ptr()];
        let mut input_bus = AudioBusBuffers {
            numChannels: self.inputs.max(1) as int32,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 { channelBuffers32: in_ptrs.as_mut_ptr() },
        };
        let mut output_bus = AudioBusBuffers {
            numChannels: self.outputs.max(1) as int32,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 { channelBuffers32: out_ptrs.as_mut_ptr() },
        };

        if let Ok(mut events) = self.events.events.lock() {
            events.clear();
            events.extend(notes.iter().map(note_event));
        }
        if let Ok(mut edits) = self.edits.try_lock() {
            self.pending.extend(edits.drain(..));
        }
        // One queue per parameter with its latest value; what does not fit
        // stays pending for the next block.
        let mut used = 0;
        let mut index = 0;
        while index < self.pending.len() {
            let (id, value) = self.pending[index];
            let same = (0..used).find(|&queue| {
                self.changes.queues[queue].id.load(Ordering::Relaxed) == id
            });
            if let Some(queue) = same {
                self.changes.queues[queue].set(id, value);
                self.pending.remove(index);
            } else if used < MAX_PARAM_CHANGES {
                self.changes.queues[used].set(id, value);
                used += 1;
                self.pending.remove(index);
            } else {
                index += 1;
            }
        }
        self.changes.count.store(used, Ordering::Relaxed);
        let events = &self.events;
        let changes = &self.changes;

        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as int32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as int32,
            numSamples: frames as int32,
            numInputs: if self.inputs > 0 { 1 } else { 0 },
            numOutputs: 1,
            inputs: if self.inputs > 0 { &mut input_bus } else { std::ptr::null_mut() },
            outputs: &mut output_bus,
            inputParameterChanges: changes
                .as_com_ref::<IParameterChanges>()
                .map_or(std::ptr::null_mut(), |c| c.as_ptr()),
            outputParameterChanges: std::ptr::null_mut(),
            inputEvents: events
                .as_com_ref::<IEventList>()
                .map_or(std::ptr::null_mut(), |c| c.as_ptr()),
            outputEvents: std::ptr::null_mut(),
            processContext: std::ptr::null_mut(),
        };
        // SAFETY: every pointer in `data` outlives this call and the buffers
        // hold `frames` samples.
        let result = unsafe { self.processor.process(&mut data) };
        let silent = output_bus.silenceFlags;
        for channel in 0..2 {
            if silent & (1 << channel) != 0 {
                self.scratch_out[channel][..frames].fill(0.0);
            }
        }
        if ok(result) {
            left[..frames].copy_from_slice(&self.scratch_out[0][..frames]);
            right[..frames].copy_from_slice(
                &self.scratch_out[if self.outputs > 1 { 1 } else { 0 }][..frames],
            );
        }
    }

    /// Tells the plug-in the sound card now runs at another rate. The plug-in
    /// is stopped, set up again and restarted, which keeps its settings and
    /// its open editor.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        if !self.running || (sample_rate - self.sample_rate).abs() < 0.5 {
            return;
        }
        let mut setup = ProcessSetup {
            processMode: ProcessModes_::kRealtime as int32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as int32,
            maxSamplesPerBlock: MAX_BLOCK as int32,
            sampleRate: sample_rate,
        };
        // SAFETY: plain COM calls in the order the specification gives for a
        // change of setup: stop, deactivate, set up, activate, start.
        unsafe {
            self.processor.setProcessing(0);
            self.component.setActive(0);
            self.processor.setupProcessing(&mut setup);
            self.component.setActive(1);
            self.processor.setProcessing(1);
        }
        self.sample_rate = sample_rate;
    }

    /// The plug-in's saved settings.
    pub fn save_state(&self) -> Vec<u8> {
        let stream = MemoryStream::new(Vec::new());
        let Some(stream_ref) = stream.as_com_ref::<IBStream>() else {
            return Vec::new();
        };
        // SAFETY: the stream outlives the call.
        unsafe {
            if !ok(self.component.getState(stream_ref.as_ptr())) {
                return Vec::new();
            }
        }
        stream.state.lock().map(|state| state.0.clone()).unwrap_or_default()
    }

    /// Restores settings saved by `save_state`.
    pub fn load_state(&self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let stream = MemoryStream::new(data.to_vec());
        let Some(stream_ref) = stream.as_com_ref::<IBStream>() else {
            return;
        };
        // SAFETY: the stream outlives the calls.
        unsafe {
            self.component.setState(stream_ref.as_ptr());
            if let Ok(mut state) = stream.state.lock() {
                state.1 = 0;
            }
            if let Some(controller) = &self.controller {
                controller.setComponentState(stream_ref.as_ptr());
            }
        }
    }

    /// What the plug-in's own window has done to its parameters.
    pub fn gestures(&self) -> Arc<Gestures> {
        self.gestures.clone()
    }

    /// What the UI thread needs to show and change the plug-in's parameters,
    /// without holding the lock the audio thread runs the plug-in under.
    pub fn controller_handle(&self) -> Option<ControllerHandle> {
        self.controller.clone().map(|controller| ControllerHandle {
            controller,
            edits: self.edits.clone(),
        })
    }
}

/// The plug-in's controller and the queue that carries parameter changes
/// over to its processor. Only for the thread that owns the editor.
pub struct ControllerHandle {
    controller: ComPtr<IEditController>,
    edits: EditQueue,
}

impl Clone for ControllerHandle {
    fn clone(&self) -> Self {
        Self {
            controller: self.controller.clone(),
            edits: self.edits.clone(),
        }
    }
}

impl ControllerHandle {
    pub fn controller(&self) -> &ComPtr<IEditController> {
        &self.controller
    }

    /// The parameters the plug-in shows the user, for a generic editor.
    pub fn parameters(&self) -> Vec<ParameterDesc> {
        let controller = &self.controller;
        let mut found = Vec::new();
        // SAFETY: COM calls on the controller; `info` is a valid out-parameter.
        unsafe {
            for index in 0..controller.getParameterCount().max(0) {
                let mut info: ParameterInfo = std::mem::zeroed();
                if !ok(controller.getParameterInfo(index, &mut info)) {
                    continue;
                }
                let flags = info.flags;
                let hidden = flags & ParameterInfo_::ParameterFlags_::kIsHidden != 0;
                let read_only = flags & ParameterInfo_::ParameterFlags_::kIsReadOnly != 0;
                let program = flags & ParameterInfo_::ParameterFlags_::kIsProgramChange != 0;
                if hidden || program {
                    continue;
                }
                found.push(ParameterDesc {
                    id: info.id,
                    title: wide_to_string(&info.title),
                    units: wide_to_string(&info.units),
                    steps: info.stepCount.max(0) as u32,
                    default: info.defaultNormalizedValue.clamp(0.0, 1.0),
                    read_only,
                });
            }
        }
        found
    }

    pub fn value(&self, id: u32) -> f64 {
        // SAFETY: plain COM call.
        unsafe { self.controller.getParamNormalized(id) }
    }

    /// The value as the plug-in writes it ("-6.0 dB").
    pub fn text(&self, id: u32, normalized: f64) -> String {
        let mut text: String128 = [0; 128];
        // SAFETY: `text` is a valid out-parameter.
        unsafe {
            if ok(self.controller.getParamStringByValue(id, normalized, &mut text)) {
                return wide_to_string(&text);
            }
        }
        format!("{normalized:.2}")
    }

    /// Sets a parameter from the generic editor, in the plug-in's controller
    /// and, through the edit queue, in its processor.
    pub fn set(&self, id: u32, normalized: f64) {
        let normalized = normalized.clamp(0.0, 1.0);
        // SAFETY: plain COM call.
        unsafe {
            self.controller.setParamNormalized(id, normalized);
        }
        if let Ok(mut edits) = self.edits.lock() {
            edits.push((id, normalized));
        }
    }
}

/// One parameter of a plug-in.
#[derive(Clone, Debug)]
pub struct ParameterDesc {
    pub id: u32,
    pub title: String,
    pub units: String,
    /// 0 for a continuous parameter, otherwise the number of steps.
    #[allow(dead_code)]
    pub steps: u32,
    /// The value the plug-in starts with, 0.0 to 1.0.
    pub default: f64,
    pub read_only: bool,
}

fn wide_to_string(text: &[TChar]) -> String {
    let units: Vec<u16> = text.iter().take_while(|&&unit| unit != 0).map(|&unit| unit as u16).collect();
    String::from_utf16_lossy(&units)
}

impl Drop for Instance {
    fn drop(&mut self) {
        // SAFETY: shutting the plug-in down in the order the specification
        // gives: stop, deactivate, disconnect, terminate.
        unsafe {
            if self.running {
                self.processor.setProcessing(0);
                self.component.setActive(0);
                self.running = false;
            }
            if self.separate_controller {
                if let (Some(a), Some(controller)) = (self.component.cast::<IConnectionPoint>(), &self.controller)
                    && let Some(b) = controller.cast::<IConnectionPoint>()
                {
                    a.disconnect(b.as_ptr());
                    b.disconnect(a.as_ptr());
                }
                if let Some(controller) = &self.controller {
                    controller.setComponentHandler(std::ptr::null_mut());
                    controller.terminate();
                }
            }
            self.component.terminate();
        }
    }
}

// ----------------------------------------------------------------- editor

struct PlugFrame {
    requested: Arc<Mutex<Option<(i32, i32)>>>,
}

impl Class for PlugFrame {
    type Interfaces = (IPlugFrame,);
}

impl IPlugFrameTrait for PlugFrame {
    unsafe fn resizeView(&self, view: *mut IPlugView, new_size: *mut ViewRect) -> tresult {
        let size = unsafe { &*new_size };
        if let Ok(mut requested) = self.requested.lock() {
            *requested = Some((size.right - size.left, size.bottom - size.top));
        }
        if let Some(view) = unsafe { ComRef::from_raw(view) } {
            let mut rect = unsafe { *new_size };
            unsafe { view.onSize(&mut rect) };
        }
        kResultOk
    }
}

/// A plug-in's editor view, ready to be put inside a native window.
pub struct Editor {
    view: ComPtr<IPlugView>,
    _frame: ComWrapper<PlugFrame>,
    requested: Arc<Mutex<Option<(i32, i32)>>>,
    attached: bool,
}

impl Editor {
    pub fn new(controller: &ComPtr<IEditController>) -> Result<Editor, String> {
        // SAFETY: COM calls on the controller the plug-in gave out.
        unsafe {
            let view = ComPtr::<IPlugView>::from_raw(controller.createView(c"editor".as_ptr()))
                .ok_or("the plug-in has no editor")?;
            let platform = if cfg!(target_os = "windows") { kPlatformTypeHWND } else { kPlatformTypeX11EmbedWindowID };
            if !ok(view.isPlatformTypeSupported(platform)) {
                return Err("the plug-in's editor does not support this platform's windows".to_owned());
            }
            let requested = Arc::new(Mutex::new(None));
            let frame = ComWrapper::new(PlugFrame { requested: requested.clone() });
            if let Some(frame_ref) = frame.as_com_ref::<IPlugFrame>() {
                view.setFrame(frame_ref.as_ptr());
            }
            Ok(Editor { view, _frame: frame, requested, attached: false })
        }
    }

    /// The size the editor wants, in pixels.
    pub fn size(&self) -> (i32, i32) {
        let mut rect = ViewRect { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: `rect` is a valid out-parameter.
        unsafe { self.view.getSize(&mut rect) };
        ((rect.right - rect.left).max(50), (rect.bottom - rect.top).max(50))
    }

    /// Puts the editor inside a native window (an HWND on Windows).
    pub fn attach(&mut self, parent: *mut c_void) -> Result<(), String> {
        let platform = if cfg!(target_os = "windows") { kPlatformTypeHWND } else { kPlatformTypeX11EmbedWindowID };
        // SAFETY: `parent` is a live native window handle owned by the caller.
        let result = unsafe { self.view.attached(parent, platform) };
        if ok(result) {
            self.attached = true;
            Ok(())
        } else {
            Err("the plug-in's editor could not open".to_owned())
        }
    }

    /// A size the plug-in asked for since the last call.
    pub fn take_resize_request(&self) -> Option<(i32, i32)> {
        self.requested.lock().ok().and_then(|mut requested| requested.take())
    }

    /// Whether the editor can be made another size than its own.
    pub fn can_resize(&self) -> bool {
        // SAFETY: plain COM call.
        unsafe { self.view.canResize() == kResultTrue }
    }

    pub fn resize(&self, width: i32, height: i32) {
        let mut rect = ViewRect { left: 0, top: 0, right: width, bottom: height };
        // SAFETY: `rect` is a valid in-parameter.
        unsafe { self.view.onSize(&mut rect) };
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        // SAFETY: detach before the window the view lives in goes away.
        unsafe {
            if self.attached {
                self.view.removed();
            }
            self.view.setFrame(std::ptr::null_mut());
        }
    }
}

#[allow(dead_code)]
fn _unused(_: &CStr) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests that need a real plug-in run when `BW_TEST_VST3` names a
    /// `.vst3` bundle for the vst3 crate's `gain` example (a stereo gain
    /// whose one parameter starts at 1.0). They pass quietly without one.
    fn fixture() -> Option<Arc<Module>> {
        let path = std::env::var_os("BW_TEST_VST3")?;
        Some(Module::load(Path::new(&path)).expect("the test plug-in should load"))
    }

    #[test]
    fn class_ids_round_trip_through_hex() {
        let id: TUID = std::array::from_fn(|index| (index as u8 * 17) as c_char);
        let hex = class_id_to_hex(&id);
        assert_eq!(hex.len(), 32);
        assert_eq!(class_id_from_hex(&hex), Some(id));
        assert_eq!(class_id_from_hex("zz"), None);
    }

    #[test]
    fn a_missing_bundle_is_an_error_not_a_crash() {
        assert!(Module::load(Path::new("/nonexistent/Nothing.vst3")).is_err());
    }

    #[test]
    fn the_factory_lists_the_audio_class() {
        let Some(module) = fixture() else { return };
        let classes = module.classes();
        assert_eq!(classes.len(), 1);
        assert!(classes[0].name.starts_with("Gain"));
        assert_eq!(classes[0].class_id.len(), 32);
        assert_ne!(kind_of(&classes[0]), PluginKind::Instrument);
    }

    #[test]
    fn an_effect_changes_audio_through_its_parameter_and_keeps_its_state() {
        let Some(module) = fixture() else { return };
        let mut instance = Instance::create(&module, "", 48_000.0).expect("instance");
        assert_eq!((instance.inputs, instance.outputs), (2, 2));

        let mut left = vec![0.5_f32; 64];
        let mut right = vec![-0.5_f32; 64];
        instance.process(&mut left, &mut right, &[]);
        assert!((left[10] - 0.5).abs() < 1e-6 && (right[10] + 0.5).abs() < 1e-6);

        let handle = instance.controller_handle().expect("controller");
        let params = handle.parameters();
        assert_eq!(params.len(), 1);
        assert!((handle.value(params[0].id) - 1.0).abs() < 1e-9);
        handle.set(params[0].id, 0.5);

        let mut left = vec![0.5_f32; 64];
        let mut right = vec![0.5_f32; 64];
        instance.process(&mut left, &mut right, &[]);
        assert!(left[10] < 0.5, "the gain parameter should lower the level: {}", left[10]);

        // The example plug-in keeps no state of its own, so saving gives
        // nothing and loading nothing must not disturb it.
        assert!(instance.save_state().is_empty());
        instance.load_state(&[]);
    }

    #[test]
    fn the_memory_stream_reads_writes_and_seeks_like_a_plug_in_expects() {
        let stream = MemoryStream::new(Vec::new());
        let bytes = [1_u8, 2, 3, 4, 5];
        let mut written = 0;
        // SAFETY: the buffers outlive the calls.
        unsafe {
            assert_eq!(stream.write(bytes.as_ptr() as *mut c_void, 5, &mut written), kResultOk);
            assert_eq!(written, 5);
            let mut at = 0;
            assert_eq!(stream.seek(1, 0, &mut at), kResultOk);
            let mut back = [0_u8; 8];
            let mut read = 0;
            assert_eq!(stream.read(back.as_mut_ptr() as *mut c_void, 8, &mut read), kResultOk);
            assert_eq!(read, 4);
            assert_eq!(&back[..4], &[2, 3, 4, 5]);
            assert_eq!(stream.seek(-2, 2, &mut at), kResultOk);
            assert_eq!(at, 3);
            assert_eq!(stream.seek(-9, 1, &mut at), kInvalidArgument);
        }
        assert_eq!(stream.state.lock().unwrap().0, bytes);
    }

    #[test]
    fn the_host_builds_the_graph_and_the_audio_thread_runs_it() {
        let Some(_module) = fixture() else { return };
        let path = std::env::var("BW_TEST_VST3").unwrap();
        let mut project = crate::core::BwxProject::demo();
        let slot = |id| crate::core::SoundChainSlot {
            id,
            name: "Gain".to_owned(),
            path: path.clone(),
            ..Default::default()
        };
        let channel = project.tracks[0].channel as usize;
        project.tracks[0].sound_chain.push(slot(1));
        project.master_chain.push(slot(2));
        let mut host = super::super::host::PluginHost::new();
        host.set_sample_rate(48_000.0);
        let graph = host.build_graph(&project);
        assert_eq!(graph.channels[channel].len(), 1);
        assert_eq!(graph.master.len(), 1);

        project.tracks[0].fx_bypassed = true;
        project.master_chain[0].bypassed = true;
        let graph = host.build_graph(&project);
        assert!(graph.is_empty(), "bypassed chains stay out of the graph");
    }
}
