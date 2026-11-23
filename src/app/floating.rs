//! Windows that float above the program: real windows of the operating
//! system, so they can be dragged anywhere on the desktop and over (or
//! beyond) the program's window instead of being stuck inside it.
//!
//! Each one is a deferred viewport: it is drawn and presented from its own
//! redraw, never inside the main window's frame. An immediate viewport is
//! presented inside its parent's frame, so every open window put another wait
//! for the display into the main window's frame time. A window repaints for
//! its own input, its own timer, or when another window asks it to.

use std::cell::RefCell;
use std::hash::Hash;
use std::rc::{Rc, Weak};
use std::thread::ThreadId;

use super::*;
use crate::plugins::native_window;

/// The app as shared with the windows that draw themselves.
type SharedApp = Rc<RefCell<BetterWriterApp>>;
pub(super) type AppHandle = Weak<RefCell<BetterWriterApp>>;

/// How often an open window looks at the app again when nothing wakes it.
const WINDOW_REFRESH: Duration = Duration::from_secs(1);

/// A value only the main thread may touch, in a form egui's deferred
/// viewports accept (they ask for `Send + Sync`).
struct MainThread<T> {
    value: T,
    owner: ThreadId,
}

// SAFETY: eframe draws every viewport, deferred ones included, on the thread
// that runs the event loop, which is also the thread that owns the app.
// `get` asserts it, so a callback run anywhere else panics instead of
// touching the value.
unsafe impl<T> Send for MainThread<T> {}
unsafe impl<T> Sync for MainThread<T> {}

impl<T> MainThread<T> {
    fn new(value: T) -> Self {
        Self { value, owner: std::thread::current().id() }
    }

    fn get(&self) -> &T {
        assert_eq!(std::thread::current().id(), self.owner, "a window was drawn off the main thread");
        &self.value
    }
}

/// Puts the app behind a shared handle so windows can draw from it, and
/// forwards the main window's frame to it.
pub(crate) struct Shell(SharedApp);

impl Shell {
    pub(crate) fn new(app: BetterWriterApp) -> Self {
        let shared = Rc::new(RefCell::new(app));
        shared.borrow_mut().handle = Rc::downgrade(&shared);
        Self(shared)
    }
}

impl eframe::App for Shell {
    fn ui(&mut self, root: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.0.borrow_mut().ui(root, frame);
    }
}

/// Keys that work in every window of the program, not only the main one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowShortcut {
    Undo,
    Redo,
    TogglePlayback,
}

/// The shortcuts pressed in the window being drawn.
fn pressed_shortcuts(ctx: &egui::Context) -> Vec<WindowShortcut> {
    ctx.input(|input| {
        let mut pressed = Vec::new();
        if input.modifiers.command && input.key_pressed(egui::Key::Z) {
            pressed.push(if input.modifiers.shift { WindowShortcut::Redo } else { WindowShortcut::Undo });
        }
        if input.modifiers.command && input.key_pressed(egui::Key::Y) {
            pressed.push(WindowShortcut::Redo);
        }
        if !input.modifiers.command && !input.modifiers.alt && input.key_pressed(egui::Key::Space) {
            pressed.push(WindowShortcut::TogglePlayback);
        }
        pressed
    })
}

/// Input that can change what the program shows, as opposed to the pointer
/// merely moving over a window.
pub(super) fn is_edit_event(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key { pressed: true, .. }
            | egui::Event::PointerButton { .. }
            | egui::Event::MouseWheel { .. }
            | egui::Event::Text(_)
            | egui::Event::Paste(_)
            | egui::Event::Cut
    )
}

/// What a window is made of: its identity and where it starts.
pub(super) struct WindowSpec<'a> {
    pub id: egui::ViewportId,
    pub title: &'a str,
    pub size: Vec2,
    pub min_size: Vec2,
}

impl<'a> WindowSpec<'a> {
    /// `id` names the window for good; the title may change.
    pub fn new(id: impl Hash, title: &'a str, size: Vec2, min_size: Vec2) -> Self {
        Self { id: egui::ViewportId::from_hash_of(id), title, size, min_size }
    }
}

/// One window's callbacks, as the viewport keeps them.
struct Task<C, X> {
    app: AppHandle,
    title: String,
    contents: C,
    on_close: X,
}

impl<C, X> Task<C, X>
where
    C: Fn(&mut BetterWriterApp, &mut egui::Ui),
    X: Fn(&mut BetterWriterApp, &egui::Context),
{
    fn frame(&self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let Some(shared) = self.app.upgrade() else {
            return;
        };
        // Only the main window's frame runs with the app borrowed, and a
        // window is never drawn inside it.
        let Ok(mut app) = shared.try_borrow_mut() else {
            ctx.request_repaint();
            return;
        };
        let app = &mut *app;
        app.adopt_native_window(&ctx, &self.title);
        if ui.input(|state| state.viewport().close_requested()) {
            (self.on_close)(app, &ctx);
        } else {
            egui::CentralPanel::default().show_inside(ui, |ui| (self.contents)(app, ui));
            app.carry_out_window_shortcuts(&ctx);
        }
        app.wake_after_window_frame(&ctx);
    }
}

impl BetterWriterApp {
    /// Declares a window of its own, called `spec.title`. It must be declared
    /// on every frame of the main window while it should stay open. `contents`
    /// draws it, in a panel, from the app's state, `on_close` runs when the user closes it.
    ///
    /// Where the platform has no such windows, egui shows an ordinary window
    /// inside the program's window instead, drawn within the main frame.
    pub(super) fn floating_window(
        &mut self,
        ctx: &egui::Context,
        spec: WindowSpec<'_>,
        contents: impl Fn(&mut BetterWriterApp, &mut egui::Ui) + 'static,
        on_close: impl Fn(&mut BetterWriterApp, &egui::Context) + 'static,
    ) {
        let builder = egui::ViewportBuilder::default()
            .with_title(spec.title)
            .with_inner_size(spec.size)
            .with_min_inner_size(spec.min_size);
        self.open_windows.push(spec.id);
        if ctx.embed_viewports() {
            ctx.show_viewport_immediate(spec.id, builder, |ui, _| {
                if ui.input(|state| state.viewport().close_requested()) {
                    on_close(self, ui.ctx());
                } else {
                    egui::CentralPanel::default().show_inside(ui, |ui| contents(self, ui));
                }
            });
            return;
        }
        let task = MainThread::new(Task {
            app: self.handle.clone(),
            title: spec.title.to_owned(),
            contents,
            on_close,
        });
        ctx.show_viewport_deferred(spec.id, builder, move |ui, _| task.get().frame(ui));
    }

    /// Finds the operating-system window of the viewport being drawn and makes
    /// the program's window its owner: it stays above it and goes with it when
    /// it is minimised, like a plug-in window in a DAW.
    fn adopt_native_window(&mut self, ctx: &egui::Context, title: &str) {
        let viewport = ctx.viewport_id();
        if self.native_windows.get(&viewport).is_some_and(|window| native_window::window_exists(*window)) {
            return;
        }
        let window = native_window::find_window(title);
        if window != 0 {
            native_window::set_owner(window, self.main_window);
            self.native_windows.insert(viewport, window);
        }
    }

    /// Carries out the keys that work in every window. A window whose text
    /// field has focus keeps the keys.
    fn carry_out_window_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        for shortcut in pressed_shortcuts(ctx) {
            match shortcut {
                WindowShortcut::Undo => self.undo(),
                WindowShortcut::Redo => self.redo(),
                WindowShortcut::TogglePlayback => self.toggle_playback(),
            }
        }
    }

    /// Schedules what follows a window's frame. Input in the window may have
    /// changed what the main window and the other windows show, so they look
    /// again; a window nobody touches only looks at the app now and then.
    fn wake_after_window_frame(&self, ctx: &egui::Context) {
        ctx.request_repaint_after(WINDOW_REFRESH);
        let touched = ctx.input(|input| input.pointer.any_down() || input.events.iter().any(is_edit_event));
        if !touched {
            return;
        }
        let this = ctx.viewport_id();
        ctx.request_repaint();
        ctx.request_repaint_of(egui::ViewportId::ROOT);
        for window in self.open_windows.iter().filter(|window| **window != this) {
            ctx.request_repaint_of(*window);
        }
    }

    /// After the main window's own frame: input there may have changed what
    /// the open windows show.
    pub(super) fn wake_windows_after_input(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.events.iter().any(is_edit_event)) {
            for window in &self.open_windows {
                ctx.request_repaint_of(*window);
            }
        }
        let open = &self.open_windows;
        self.native_windows.retain(|viewport, _| open.contains(viewport));
    }
}
