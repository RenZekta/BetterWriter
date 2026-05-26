mod app;
mod audio;
mod core;
mod format;
mod theme;

use app::BetterWriterApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
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
