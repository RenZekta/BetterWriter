//! Notes on the system clipboard as a MIDI clip, so other programs (a DAW,
//! another score editor) can paste them and BetterWriter can paste theirs.

use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext};

/// Names programs use for a MIDI file on the clipboard.
const MIDI_FORMATS: [&str; 6] = [
    "audio/midi",
    "audio/x-midi",
    "audio/mid",
    "public.midi-audio",
    "Standard MIDI File",
    "MIDI",
];

#[cfg(target_os = "macos")]
const WRITTEN_FORMAT: &str = "public.midi-audio";
#[cfg(not(target_os = "macos"))]
const WRITTEN_FORMAT: &str = "audio/midi";

/// Holds the connection to the system clipboard. On X11 the program that
/// copied has to stay connected to serve the data, so it is kept for the
/// whole run.
#[derive(Default)]
pub(super) struct SystemClipboard {
    context: Option<ClipboardContext>,
}

impl SystemClipboard {
    fn context(&mut self) -> Option<&ClipboardContext> {
        if self.context.is_none() {
            self.context = ClipboardContext::new().ok();
        }
        self.context.as_ref()
    }

    /// Puts a MIDI clip on the clipboard as a MIDI file in memory and as a
    /// file on disk, with `text` for text fields. `false` when the system
    /// clipboard cannot be reached.
    pub fn put_midi(&mut self, midi: Vec<u8>, text: String) -> bool {
        let path = std::env::temp_dir()
            .join("BetterWriter")
            .join("BetterWriter notes.mid");
        let file_written = path
            .parent()
            .is_some_and(|folder| std::fs::create_dir_all(folder).is_ok())
            && std::fs::write(&path, &midi).is_ok();
        let mut contents = vec![
            ClipboardContent::Other(WRITTEN_FORMAT.to_owned(), midi),
            ClipboardContent::Text(text),
        ];
        if file_written {
            contents.push(ClipboardContent::Files(vec![path.to_string_lossy().into_owned()]));
        }
        self.context()
            .is_some_and(|context| context.set(contents).is_ok())
    }

    /// The MIDI file another program put on the clipboard, if any.
    pub fn midi(&mut self) -> Option<Vec<u8>> {
        let context = self.context()?;
        let formats = context.available_formats().unwrap_or_default();
        for name in MIDI_FORMATS {
            if formats.iter().any(|format| format == name)
                && let Ok(bytes) = context.get_buffer(name)
                && !bytes.is_empty()
            {
                return Some(bytes);
            }
        }
        let files = context.get_files().ok()?;
        let path = files.iter().find(|path| {
            let lower = path.to_lowercase();
            lower.ends_with(".mid") || lower.ends_with(".midi")
        })?;
        std::fs::read(path.strip_prefix("file://").unwrap_or(path)).ok()
    }
}
