pub mod midi;
pub use midi::MidiIn;

mod ui;

const APP_NAME: &str = "MIDI sniffer";

fn main() {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Debug)
        .init();

    match ui::App::try_new(APP_NAME).map(|ui| {
        let options = eframe::NativeOptions::default();
        eframe::run_native(Box::new(ui), options);
    }) {
        Ok(()) => log::info!("Exiting"),
        Err(err) => {
            use std::error::Error;

            log::error!("Error: {}", err);
            if let Some(source) = err.source() {
                log::error!("\t{}", source)
            }
        }
    }
}
