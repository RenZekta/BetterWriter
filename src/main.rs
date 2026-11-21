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

use app::BetterWriterApp;

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
        // Present at the display's own refresh rate; nothing in the app caps it.
        vsync: true,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1420.0, 880.0])
            .with_min_inner_size([980.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "BetterWriter",
        options,
        Box::new(|cc| Ok(Box::new(BetterWriterApp::new(cc)))),
    )
}
