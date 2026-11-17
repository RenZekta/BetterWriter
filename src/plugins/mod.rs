//! Plug-in discovery and hosting.

pub mod host;
pub mod native_window;
pub mod scan;
pub mod vst3;

pub use scan::{PluginKind, PluginScanner, ScannedPlugin};
