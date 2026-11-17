//! Finds installed plug-ins (VST3, VST2, CLAP, LV2, AU) in the standard
//! folders, off the UI thread, and keeps the result in a small text file so
//! the list is there at the next launch before the new scan finishes.
//!
//! A VST3 bundle's kind (instrument or effect) and developer come from its
//! factory. Loading arbitrary plug-in code can crash, so that is done in a
//! short-lived child process (`--probe-vst3`) and remembered per file.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime};

use crate::core::{PluginFormat, SoundChainSlot};

/// Subfolders below a plug-in folder that are searched (vendors often keep
/// their plug-ins one or two levels down).
const MAX_SCAN_DEPTH: usize = 4;
/// A plug-in that takes longer than this to report is skipped.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
/// How many finds the scan batches before showing them.
const PROGRESS_BATCH: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginKind {
    Instrument,
    Effect,
    /// Nothing in the file says; shown by format alone.
    Unclassified,
}

impl PluginKind {
    fn key(self) -> &'static str {
        match self {
            PluginKind::Instrument => "instrument",
            PluginKind::Effect => "effect",
            PluginKind::Unclassified => "unclassified",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "instrument" => Some(PluginKind::Instrument),
            "effect" => Some(PluginKind::Effect),
            "unclassified" => Some(PluginKind::Unclassified),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannedPlugin {
    pub name: String,
    pub path: PathBuf,
    pub format: PluginFormat,
    pub kind: PluginKind,
    /// The developer; empty when nothing names one.
    pub vendor: String,
    /// Which class of a bundle that holds several, as hex; empty otherwise.
    pub class_id: String,
}

impl ScannedPlugin {
    /// "VST3i: Name" for an instrument, "VST3: Name" otherwise.
    pub fn list_label(&self) -> String {
        self.to_slot().list_label()
    }

    pub fn to_slot(&self) -> SoundChainSlot {
        SoundChainSlot {
            id: 0,
            name: self.name.clone(),
            path: self.path.display().to_string(),
            is_instrument: self.kind == PluginKind::Instrument,
            bypassed: false,
            format: self.format,
            vendor: self.vendor.clone(),
            class_id: self.class_id.clone(),
            label: String::new(),
            state: Vec::new(),
        }
    }

    /// The folder name the browser files this plug-in under.
    pub fn developer(&self) -> &str {
        if self.vendor.is_empty() { "Unknown" } else { &self.vendor }
    }
}

/// A folder to search and the format of what it holds.
pub fn scan_roots() -> Vec<(PluginFormat, PathBuf)> {
    let mut roots: Vec<(PluginFormat, PathBuf)> = Vec::new();
    let env_dir = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let mut add = |format: PluginFormat, path: PathBuf| {
        if !roots.iter().any(|(f, p)| *f == format && *p == path) {
            roots.push((format, path));
        }
    };
    if cfg!(target_os = "windows") {
        let common = [
            env_dir("COMMONPROGRAMFILES"),
            env_dir("COMMONPROGRAMFILES(X86)"),
            Some(PathBuf::from(r"C:\Program Files\Common Files")),
        ];
        for dir in common.into_iter().flatten() {
            add(PluginFormat::Vst3, dir.join("VST3"));
            add(PluginFormat::Vst2, dir.join("VST2"));
            add(PluginFormat::Clap, dir.join("CLAP"));
            add(PluginFormat::Lv2, dir.join("LV2"));
        }
        if let Some(local) = env_dir("LOCALAPPDATA") {
            add(PluginFormat::Vst3, local.join("Programs/Common/VST3"));
            add(PluginFormat::Clap, local.join("Programs/Common/CLAP"));
        }
        if let Some(roaming) = env_dir("APPDATA") {
            add(PluginFormat::Lv2, roaming.join("LV2"));
        }
        for program_files in [
            env_dir("PROGRAMFILES"),
            env_dir("PROGRAMFILES(X86)"),
            Some(PathBuf::from(r"C:\Program Files")),
        ]
        .into_iter()
        .flatten()
        {
            add(PluginFormat::Vst2, program_files.join("VSTPlugins"));
            add(PluginFormat::Vst2, program_files.join("Steinberg/VSTPlugins"));
        }
    } else if cfg!(target_os = "macos") {
        let library = PathBuf::from("/Library/Audio/Plug-Ins");
        let user = env_dir("HOME").map(|home| home.join("Library/Audio/Plug-Ins"));
        for base in std::iter::once(library).chain(user) {
            add(PluginFormat::Vst3, base.join("VST3"));
            add(PluginFormat::Vst2, base.join("VST"));
            add(PluginFormat::Clap, base.join("CLAP"));
            add(PluginFormat::Lv2, base.join("LV2"));
            add(PluginFormat::Au, base.join("Components"));
        }
    } else {
        let home = env_dir("HOME");
        let home_dir = |sub: &str| home.as_ref().map(|dir| dir.join(sub));
        for dir in home_dir(".vst3").into_iter().chain([
            PathBuf::from("/usr/lib/vst3"),
            PathBuf::from("/usr/local/lib/vst3"),
        ]) {
            add(PluginFormat::Vst3, dir);
        }
        for dir in home_dir(".vst").into_iter().chain([
            PathBuf::from("/usr/lib/vst"),
            PathBuf::from("/usr/local/lib/vst"),
        ]) {
            add(PluginFormat::Vst2, dir);
        }
        for dir in home_dir(".clap").into_iter().chain([
            PathBuf::from("/usr/lib/clap"),
            PathBuf::from("/usr/local/lib/clap"),
        ]) {
            add(PluginFormat::Clap, dir);
        }
        for dir in home_dir(".lv2").into_iter().chain([
            PathBuf::from("/usr/lib/lv2"),
            PathBuf::from("/usr/local/lib/lv2"),
        ]) {
            add(PluginFormat::Lv2, dir);
        }
    }
    // The format's own search-path variables.
    for (format, variable) in [(PluginFormat::Clap, "CLAP_PATH"), (PluginFormat::Lv2, "LV2_PATH")] {
        if let Some(paths) = std::env::var_os(variable) {
            for path in std::env::split_paths(&paths) {
                add(format, path);
            }
        }
    }
    roots
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|found| found.to_str())
        .is_some_and(|found| found.eq_ignore_ascii_case(extension))
}

/// Whether `path` is a plug-in of `format`; plug-in bundles are not searched
/// further.
fn is_plugin_path(format: PluginFormat, path: &Path) -> bool {
    match format {
        PluginFormat::Vst3 => has_extension(path, "vst3"),
        PluginFormat::Clap => has_extension(path, "clap"),
        PluginFormat::Lv2 => has_extension(path, "lv2") && path.is_dir(),
        PluginFormat::Au => has_extension(path, "component"),
        PluginFormat::Vst2 => {
            if cfg!(target_os = "windows") {
                has_extension(path, "dll")
            } else if cfg!(target_os = "macos") {
                has_extension(path, "vst")
            } else {
                has_extension(path, "so")
            }
        }
    }
}

fn collect_plugins(format: PluginFormat, dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if is_plugin_path(format, &path) {
            found.push(path);
        } else if depth < MAX_SCAN_DEPTH && path.is_dir() {
            collect_plugins(format, &path, depth + 1, found);
        }
    }
}

/// The first folder below `root` that holds the plug-in, which vendors use
/// to group their products.
fn folder_vendor(root: &Path, plugin: &Path) -> String {
    let Ok(relative) = plugin.strip_prefix(root) else {
        return String::new();
    };
    let mut parts = relative.components();
    match (parts.next(), parts.next()) {
        (Some(first), Some(_)) => first.as_os_str().to_string_lossy().into_owned(),
        _ => String::new(),
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What a VST3 factory says about one class of a bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbedClass {
    pub class_id: String,
    pub name: String,
    pub vendor: String,
    pub kind: PluginKind,
}

/// The kind a VST3 class's sub-categories ("Fx|Reverb", "Instrument|Synth")
/// declare.
pub fn kind_from_sub_categories(sub_categories: &str) -> PluginKind {
    let mut instrument = false;
    let mut effect = false;
    for category in sub_categories.split('|') {
        match category.trim() {
            "Instrument" => instrument = true,
            "Fx" => effect = true,
            _ => {}
        }
    }
    match (instrument, effect) {
        (true, _) => PluginKind::Instrument,
        (false, true) => PluginKind::Effect,
        (false, false) => PluginKind::Unclassified,
    }
}

/// Reads the vendor and sub-categories a bundle's `moduleinfo.json` declares.
/// Looking only at that file loads no plug-in code.
fn module_info_class(plugin: &Path) -> Option<ProbedClass> {
    let text = fs::read_to_string(plugin.join("Contents/Resources/moduleinfo.json")).ok()?;
    let mut instrument = false;
    let mut effect = false;
    let mut rest = text.as_str();
    while let Some(position) = rest.find("\"Sub Categories\"") {
        rest = &rest[position + "\"Sub Categories\"".len()..];
        let open = rest.find('[')?;
        let close = rest[open..].find(']')?;
        let list = &rest[open..open + close];
        instrument |= list.contains("\"Instrument\"");
        effect |= list.contains("\"Fx\"");
    }
    let kind = match (instrument, effect) {
        (true, _) => PluginKind::Instrument,
        (false, true) => PluginKind::Effect,
        (false, false) => return None,
    };
    let vendor = json_string_after(&text, "\"Vendor\"").unwrap_or_default();
    Some(ProbedClass {
        class_id: String::new(),
        name: file_stem(plugin),
        vendor,
        kind,
    })
}

fn json_string_after(text: &str, key: &str) -> Option<String> {
    let rest = &text[text.find(key)? + key.len()..];
    let start = rest.find('"')? + 1;
    let end = rest[start..].find('"')?;
    Some(rest[start..start + end].to_owned())
}

/// Runs this program as `--probe-vst3 <bundle>` and reads the classes it
/// reports. A plug-in that crashes or hangs costs only the child process.
fn probe_in_child(plugin: &Path) -> Option<Vec<ProbedClass>> {
    let exe = std::env::current_exe().ok()?;
    let mut child = Command::new(exe)
        .arg("--probe-vst3")
        .arg(plugin)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < PROBE_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let text = reader.join().ok()?;
    if !status.success() {
        return None;
    }
    let classes = parse_probe_output(&text);
    (!classes.is_empty()).then_some(classes)
}

/// The probe child prints one `class_id<TAB>vendor<TAB>sub_categories<TAB>name`
/// line per audio class.
pub fn parse_probe_output(text: &str) -> Vec<ProbedClass> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.splitn(4, '\t');
            let class_id = fields.next()?.to_owned();
            let vendor = fields.next()?.to_owned();
            let kind = kind_from_sub_categories(fields.next()?);
            let name = fields.next()?.to_owned();
            (class_id.len() == 32).then_some(ProbedClass { class_id, name, vendor, kind })
        })
        .collect()
}

pub fn format_probe_line(class: &ProbedClass, sub_categories: &str) -> String {
    format!(
        "{}\t{}\t{}\t{}",
        class.class_id, class.vendor, sub_categories, class.name
    )
}

fn modified_seconds(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn cache_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|dir| dir.join("betterwriter_plugins.txt"))
}

fn format_key(format: PluginFormat) -> &'static str {
    match format {
        PluginFormat::Vst3 => "vst3",
        PluginFormat::Vst2 => "vst2",
        PluginFormat::Clap => "clap",
        PluginFormat::Lv2 => "lv2",
        PluginFormat::Au => "au",
    }
}

fn format_from_key(key: &str) -> Option<PluginFormat> {
    PluginFormat::ALL
        .into_iter()
        .find(|format| format_key(*format) == key)
}

/// A scanned plug-in with the modification time its entry was made at.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CachedPlugin {
    plugin: ScannedPlugin,
    modified: u64,
}

fn serialize(entries: &[CachedPlugin]) -> String {
    entries
        .iter()
        .map(|entry| {
            let plugin = &entry.plugin;
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                format_key(plugin.format),
                plugin.kind.key(),
                plugin.class_id,
                entry.modified,
                plugin.vendor,
                plugin.path.display(),
                plugin.name
            )
        })
        .collect()
}

fn parse(text: &str) -> Vec<CachedPlugin> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.splitn(7, '\t');
            let format = format_from_key(fields.next()?)?;
            let kind = PluginKind::from_key(fields.next()?)?;
            let class_id = fields.next()?.to_owned();
            let modified = fields.next()?.parse().ok()?;
            let vendor = fields.next()?.to_owned();
            let path = PathBuf::from(fields.next()?);
            let name = fields.next()?.to_owned();
            Some(CachedPlugin {
                plugin: ScannedPlugin { name, path, format, kind, vendor, class_id },
                modified,
            })
        })
        .collect()
}

fn load_cache() -> Vec<CachedPlugin> {
    cache_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

fn save_cache(entries: &[CachedPlugin]) {
    if let Some(path) = cache_path() {
        let _ = fs::write(path, serialize(entries));
    }
}

/// LV2 bundles name their kind in their Turtle files.
fn lv2_kind(bundle: &Path) -> PluginKind {
    let Ok(entries) = fs::read_dir(bundle) else {
        return PluginKind::Unclassified;
    };
    let mut effect = false;
    for entry in entries.flatten().take(8) {
        let path = entry.path();
        if !has_extension(&path, "ttl") {
            continue;
        }
        let mut text = String::new();
        if fs::File::open(&path)
            .and_then(|file| file.take(1 << 20).read_to_string(&mut text))
            .is_err()
        {
            continue;
        }
        if text.contains("InstrumentPlugin") {
            return PluginKind::Instrument;
        }
        effect |= text.contains("lv2:Plugin") || text.contains("lv2core#Plugin");
    }
    if effect { PluginKind::Effect } else { PluginKind::Unclassified }
}

/// The entries one plug-in file adds to the list.
fn describe(
    format: PluginFormat,
    root: &Path,
    path: &Path,
    cache: &HashMap<PathBuf, Vec<CachedPlugin>>,
    probe: bool,
) -> Vec<CachedPlugin> {
    let modified = modified_seconds(path);
    let folder = folder_vendor(root, path);
    let entry = |name: String, kind: PluginKind, vendor: String, class_id: String| CachedPlugin {
        plugin: ScannedPlugin {
            name,
            path: path.to_path_buf(),
            format,
            kind,
            vendor: if vendor.is_empty() { folder.clone() } else { vendor },
            class_id,
        },
        modified,
    };
    match format {
        PluginFormat::Vst3 => {
            if let Some(known) = cache.get(path)
                && known.iter().all(|cached| cached.modified == modified)
                && !known.is_empty()
            {
                return known.clone();
            }
            if let Some(class) = module_info_class(path) {
                return vec![entry(class.name, class.kind, class.vendor, class.class_id)];
            }
            let classes = if probe { probe_in_child(path) } else { None };
            match classes {
                Some(classes) if classes.len() == 1 => {
                    let class = &classes[0];
                    vec![entry(
                        file_stem(path),
                        class.kind,
                        class.vendor.clone(),
                        String::new(),
                    )]
                }
                Some(classes) => classes
                    .into_iter()
                    .map(|class| entry(class.name, class.kind, class.vendor, class.class_id))
                    .collect(),
                None => vec![entry(
                    file_stem(path),
                    PluginKind::Unclassified,
                    String::new(),
                    String::new(),
                )],
            }
        }
        PluginFormat::Lv2 => vec![entry(
            file_stem(path),
            lv2_kind(path),
            String::new(),
            String::new(),
        )],
        _ => vec![entry(
            file_stem(path),
            PluginKind::Unclassified,
            String::new(),
            String::new(),
        )],
    }
}

/// Scans `roots`, calling `progress` with the list so far every few finds.
/// `probe` lets VST3 bundles without a `moduleinfo.json` be loaded in a child
/// process to read their factory.
fn scan_with(
    roots: &[(PluginFormat, PathBuf)],
    probe: bool,
    previous: &[CachedPlugin],
    mut progress: impl FnMut(&[CachedPlugin]),
) -> Vec<CachedPlugin> {
    let mut cache: HashMap<PathBuf, Vec<CachedPlugin>> = HashMap::new();
    for entry in previous {
        cache
            .entry(entry.plugin.path.clone())
            .or_default()
            .push(entry.clone());
    }
    let mut paths: Vec<(PluginFormat, PathBuf, PathBuf)> = Vec::new();
    for (format, root) in roots {
        let mut found = Vec::new();
        collect_plugins(*format, root, 0, &mut found);
        paths.extend(found.into_iter().map(|path| (*format, root.clone(), path)));
    }
    paths.sort_by(|a, b| a.2.cmp(&b.2));
    paths.dedup_by(|a, b| a.2 == b.2);

    let mut entries: Vec<CachedPlugin> = Vec::new();
    let mut since_progress = 0;
    for (format, root, path) in &paths {
        entries.extend(describe(*format, root, path, &cache, probe));
        since_progress += 1;
        if since_progress >= PROGRESS_BATCH {
            since_progress = 0;
            progress(&entries);
        }
    }
    entries.sort_by_key(|entry| entry.plugin.name.to_lowercase());
    entries
}

/// Scans without loading any plug-in code. For tests and tools.
#[allow(dead_code)]
pub fn scan(roots: &[(PluginFormat, PathBuf)]) -> Vec<ScannedPlugin> {
    scan_with(roots, false, &[], |_| {})
        .into_iter()
        .map(|entry| entry.plugin)
        .collect()
}

enum ScanMessage {
    Progress(Vec<ScannedPlugin>),
    Done(Vec<ScannedPlugin>),
}

/// The plug-in list plus the scan that refreshes it.
pub struct PluginScanner {
    plugins: Vec<ScannedPlugin>,
    pending: Option<Receiver<ScanMessage>>,
}

impl PluginScanner {
    /// Starts with the stored list and begins a scan in the background.
    pub fn start() -> Self {
        let mut scanner = Self {
            plugins: load_cache().into_iter().map(|entry| entry.plugin).collect(),
            pending: None,
        };
        scanner.rescan();
        scanner
    }

    pub fn rescan(&mut self) {
        if self.pending.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("betterwriter-plugin-scan".to_owned())
            .spawn(move || {
                let previous = load_cache();
                let plain = |entries: &[CachedPlugin]| {
                    entries.iter().map(|entry| entry.plugin.clone()).collect::<Vec<_>>()
                };
                let entries = scan_with(&scan_roots(), true, &previous, |so_far| {
                    let _ = sender.send(ScanMessage::Progress(plain(so_far)));
                });
                save_cache(&entries);
                let _ = sender.send(ScanMessage::Done(plain(&entries)));
            });
        if spawned.is_ok() {
            self.pending = Some(receiver);
        }
    }

    pub fn is_scanning(&self) -> bool {
        self.pending.is_some()
    }

    pub fn plugins(&self) -> &[ScannedPlugin] {
        &self.plugins
    }

    /// Takes what the scan has found since the last call. Returns whether
    /// the scan just finished.
    pub fn poll(&mut self) -> bool {
        let Some(receiver) = &self.pending else {
            return false;
        };
        let mut finished = false;
        loop {
            match receiver.try_recv() {
                Ok(ScanMessage::Progress(plugins)) => {
                    // A partial list would hide the stored one mid-scan;
                    // only grow what is shown.
                    if plugins.len() > self.plugins.len() {
                        self.plugins = plugins;
                    }
                }
                Ok(ScanMessage::Done(plugins)) => {
                    self.plugins = plugins;
                    finished = true;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.pending = None;
        }
        finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "betterwriter_plugin_test_{name}_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn bundle(root: &Path, name: &str, module_info: Option<&str>) -> PathBuf {
        let path = root.join(name);
        fs::create_dir_all(path.join("Contents/Resources")).unwrap();
        if let Some(info) = module_info {
            fs::write(path.join("Contents/Resources/moduleinfo.json"), info).unwrap();
        }
        path
    }

    fn info(vendor: &str, sub_categories: &str) -> String {
        format!(
            "{{\"Factory Info\":{{\"Vendor\":\"{vendor}\"}},\"Classes\":[{{\"Category\":\"Audio Module Class\",\"Sub Categories\":[{sub_categories}]}}]}}"
        )
    }

    #[test]
    fn module_info_decides_kind_and_vendor() {
        let dir = temp_dir("kinds");
        let synth = bundle(&dir, "Synth.vst3", Some(&info("Acme", "\"Instrument\",\"Synth\"")));
        let verb = bundle(&dir, "Verb.vst3", Some(&info("Acme", "\"Fx\",\"Reverb\"")));
        let bare = bundle(&dir, "Bare.vst3", None);
        let synth = module_info_class(&synth).unwrap();
        assert_eq!((synth.kind, synth.vendor.as_str()), (PluginKind::Instrument, "Acme"));
        assert_eq!(module_info_class(&verb).unwrap().kind, PluginKind::Effect);
        assert!(module_info_class(&bare).is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sub_categories_split_on_bars() {
        assert_eq!(kind_from_sub_categories("Fx|Reverb"), PluginKind::Effect);
        assert_eq!(kind_from_sub_categories("Instrument|Synth"), PluginKind::Instrument);
        assert_eq!(kind_from_sub_categories("Analyzer"), PluginKind::Unclassified);
        assert_eq!(kind_from_sub_categories("Instrument|Fx"), PluginKind::Instrument);
    }

    #[test]
    fn scanning_finds_each_format_and_groups_by_vendor_folder() {
        let dir = temp_dir("scan");
        let vst3 = dir.join("vst3");
        let clap = dir.join("clap");
        bundle(&vst3, "Zeta.vst3", Some(&info("Zed Audio", "\"Fx\"")));
        let vendor = vst3.join("Vendor Folder");
        fs::create_dir_all(&vendor).unwrap();
        bundle(&vendor, "alpha.vst3", None);
        fs::create_dir_all(&clap).unwrap();
        fs::write(clap.join("Gadget.clap"), "x").unwrap();
        fs::write(vst3.join("notes.txt"), "not a plug-in").unwrap();
        let plugins = scan(&[(PluginFormat::Vst3, vst3), (PluginFormat::Clap, clap)]);
        let found: Vec<(&str, PluginFormat, &str)> = plugins
            .iter()
            .map(|p| (p.name.as_str(), p.format, p.developer()))
            .collect();
        assert_eq!(
            found,
            [
                ("alpha", PluginFormat::Vst3, "Vendor Folder"),
                ("Gadget", PluginFormat::Clap, "Unknown"),
                ("Zeta", PluginFormat::Vst3, "Zed Audio"),
            ]
        );
        assert_eq!(plugins[1].list_label(), "CLAP: Gadget");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_folder_scans_as_empty() {
        assert!(scan(&[(PluginFormat::Vst3, PathBuf::from("/definitely/not/here"))]).is_empty());
    }

    #[test]
    fn instruments_are_listed_apart_from_effects() {
        let mut plugin = ScannedPlugin {
            name: "Orion".to_owned(),
            path: PathBuf::from("/x/Orion.vst3"),
            format: PluginFormat::Vst3,
            kind: PluginKind::Effect,
            vendor: String::new(),
            class_id: String::new(),
        };
        assert_eq!(plugin.list_label(), "VST3: Orion");
        plugin.kind = PluginKind::Instrument;
        assert_eq!(plugin.list_label(), "VST3i: Orion");
        plugin.kind = PluginKind::Unclassified;
        assert_eq!(plugin.list_label(), "VST3: Orion");
    }

    #[test]
    fn the_stored_list_round_trips() {
        let entries = vec![
            CachedPlugin {
                plugin: ScannedPlugin {
                    name: "My Synth".to_owned(),
                    path: PathBuf::from("/x/My Synth.vst3"),
                    format: PluginFormat::Vst3,
                    kind: PluginKind::Instrument,
                    vendor: "Acme".to_owned(),
                    class_id: "ab".repeat(16),
                },
                modified: 42,
            },
            CachedPlugin {
                plugin: ScannedPlugin {
                    name: "Verb".to_owned(),
                    path: PathBuf::from("/x/Verb.clap"),
                    format: PluginFormat::Clap,
                    kind: PluginKind::Unclassified,
                    vendor: String::new(),
                    class_id: String::new(),
                },
                modified: 7,
            },
        ];
        assert_eq!(parse(&serialize(&entries)), entries);
    }

    #[test]
    fn probe_lines_round_trip() {
        let class = ProbedClass {
            class_id: "0123456789abcdef0123456789abcdef".to_owned(),
            name: "State Machine".to_owned(),
            vendor: "Cradle".to_owned(),
            kind: PluginKind::Instrument,
        };
        let line = format_probe_line(&class, "Instrument|Synth");
        assert_eq!(parse_probe_output(&line), [class]);
        assert!(parse_probe_output("garbage").is_empty());
    }
}
