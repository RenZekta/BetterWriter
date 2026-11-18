//! Windows that float above the program: real windows of the operating
//! system, so they can be dragged anywhere on the desktop and over (or
//! beyond) the program's window instead of being stuck inside it.

use super::*;
use crate::plugins::native_window;

/// Shows `contents` in a window of its own, called `title`. `id` names the
/// window for good (the title may change), `owner` is the program's window
/// handle, which the window is owned by: it stays above it and goes with it
/// when it is minimised. Clears `open` when the user closes the window.
///
/// Where the platform has no such windows, egui shows an ordinary window
/// inside the program's window instead.
#[allow(clippy::too_many_arguments)]
pub(super) fn floating_window(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: Vec2,
    min_size: Vec2,
    owner: usize,
    open: &mut bool,
    mut contents: impl FnMut(&mut egui::Ui),
) {
    let builder = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size(min_size);
    let owned = egui::Id::new(("floating_window_owned", id));
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of(id), builder, |ui, class| {
        if class != egui::ViewportClass::EmbeddedWindow {
            let done = ui.data(|data| data.get_temp::<bool>(owned)).unwrap_or(false);
            if !done {
                let window = native_window::find_window(title);
                if window != 0 {
                    native_window::set_owner(window, owner);
                    ui.data_mut(|data| data.insert_temp(owned, true));
                }
            }
        }
        if ui.input(|state| state.viewport().close_requested()) {
            *open = false;
        }
        egui::CentralPanel::default().show_inside(ui, |ui| contents(ui));
    });
}
