mod app;
mod audio;
mod core;
mod format;
mod fretboard;
mod icons;
mod midi_input;
mod notation;
mod plugins;
mod symbols;
mod theme;

use app::{BetterWriterApp, Shell};

fn main() -> eframe::Result<()> {
    // `--probe-vst3 <bundle>`: the plug-in scanner's child process. It loads
    // one bundle, prints its classes and exits, so a plug-in that crashes
    // while loading never takes the editor down.
    let mut args = std::env::args_os().skip(1);
    if args.next().is_some_and(|arg| arg == "--probe-vst3") {
        let code = args
            .next()
            .map_or(1, |path| plugins::vst3::probe_main(std::path::Path::new(&path)));
        std::process::exit(code);
    }

    let options = eframe::NativeOptions {
        // Frames wait for the display's refresh (adaptive sync follows the
        // display's own rate). This is the wgpu present mode; the `vsync`
        // option of `NativeOptions` only applies to the glow renderer.
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            present_mode: eframe::egui_wgpu::wgpu::PresentMode::AutoVsync,
            ..Default::default()
        },
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1420.0, 880.0])
            .with_min_inner_size([980.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "BetterWriter",
        options,
        Box::new(|cc| Ok(Box::new(Shell::new(BetterWriterApp::new(cc))))),
    )
}
