pub mod bytes;

pub mod midi;
pub use midi::MidiIn;

mod ui;

const APP_NAME: &str = "MIDI sniffer";

fn main() {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Debug)
        .init();

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "midi-sniffer",
        options,
        Box::new(|cc| Box::new(ui::App::new(APP_NAME, cc))),
    );
}
