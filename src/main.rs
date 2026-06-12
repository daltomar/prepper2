mod app;
mod ui;

use app::AblaufdatumTrackerApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Ablaufdatum-Tracker [build 20260612]")
            .with_min_inner_size([900.0, 600.0])
            .with_inner_size([1000.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Ablaufdatum-Tracker",
        native_options,
        Box::new(|cc| -> Box<dyn eframe::App> { Box::new(AblaufdatumTrackerApp::new(cc)) }),
    )
}
