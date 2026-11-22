#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("JARVIS desktop è supportato solo su macOS e Windows.");

mod app;
mod client;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("JARVIS")
            .with_icon(app::application_icon())
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([380.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "JARVIS",
        options,
        Box::new(|cc| Ok(Box::new(app::JarvisApp::new(cc)))),
    )
}
