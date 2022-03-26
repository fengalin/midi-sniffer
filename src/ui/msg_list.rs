use crossbeam_channel as channel;
use eframe::{egui, epi};
use std::{
    fmt,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::midi::{self, PortNb};

const MAX_REPETITIONS: u8 = 99;
const MAX_REPETITIONS_EXCEEDED: &str = ">99";
const STORAGE_MSG_LIST_DIR: &str = "msg_list_dir";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(feature = "save")]
    #[error("Failed to save message list: {}", .0)]
    Save(#[from] std::io::Error),
}

#[derive(Clone)]
#[cfg_attr(feature = "save", derive(serde::Serialize))]
pub struct MsgParseResult {
    #[cfg_attr(feature = "save", serde(rename = "timestamp"))]
    ts_str: Arc<str>,

    #[cfg_attr(feature = "save", serde(rename = "port"))]
    port_nb: PortNb,

    repetitions: u8,

    is_err: bool,

    #[cfg_attr(feature = "save", serde(rename = "parsed"))]
    res_str: Arc<str>,

    buffer: Arc<[u8]>,
}

impl PartialEq<midi::msg::Result> for MsgParseResult {
    fn eq(&self, other: &midi::msg::Result) -> bool {
        let other_origin = match other {
            Ok(ok) => &ok.origin,
            Err(err) => &err.origin,
        };
        self.port_nb == other_origin.port_nb && self.buffer == other_origin.buffer
    }
}

impl From<midi::msg::Result> for MsgParseResult {
    fn from(res: midi::msg::Result) -> Self {
        match res {
            Ok(ok) => {
                let mut res_str = String::new();
                write_midi_msg(&mut res_str, &ok.msg).unwrap();

                Self {
                    ts_str: format!("{}", ok.origin.ts).into(),
                    port_nb: ok.origin.port_nb,
                    repetitions: 1,
                    res_str: res_str.into(),
                    buffer: ok.origin.buffer,
                    is_err: false,
                }
            }
            Err(err) => Self {
                ts_str: format!("{}", err.origin.ts).into(),
                port_nb: err.origin.port_nb,
                repetitions: 1,
                res_str: format!("{}", err.err).into(),
                buffer: err.origin.buffer,
                is_err: true,
            },
        }
    }
}

pub enum Status {
    Unchanged,
    Updated,
}

impl Status {
    fn updated(&mut self) {
        *self = Status::Updated
    }

    pub fn was_updated(&self) -> bool {
        matches!(self, Status::Updated)
    }
}

pub struct MsgListWidget {
    pub list: Vec<Arc<MsgParseResult>>,
    follows_cursor: bool,
    err_tx: channel::Sender<super::app::Error>,
    msg_list_dir: Arc<Mutex<PathBuf>>,
}

impl MsgListWidget {
    pub fn new(err_tx: channel::Sender<super::app::Error>) -> Self {
        Self {
            list: Vec::new(),
            follows_cursor: true,
            err_tx,
            msg_list_dir: Arc::new(Mutex::new(PathBuf::from("."))),
        }
    }
}

impl MsgListWidget {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.follows_cursor, "Follow");
                ui.add_enabled_ui(!self.list.is_empty(), |ui| {
                    if ui.button("Clear").clicked() {
                        self.list.clear();
                    }
                    #[cfg(feature = "save")]
                    if ui.button("Save").clicked() {
                        self.save_list();
                    }
                });
            });

            ui.separator();
            egui::ScrollArea::both().show(ui, |ui| {
                egui::Grid::new("Msg List").num_columns(4).show(ui, |ui| {
                    ui.label("Timestamp");
                    ui.label("Port");
                    ui.label("Rep.");
                    ui.label("Message");
                    ui.end_row();

                    ui.separator();
                    ui.separator();
                    ui.separator();
                    ui.separator();
                    ui.end_row();

                    for msg in self.list.iter() {
                        let row_color = match msg.port_nb {
                            midi::PortNb::One => egui::Color32::from_rgb(0, 0, 0x64),
                            midi::PortNb::Two => egui::Color32::from_rgb(0, 0x48, 0),
                        };

                        let _ = ui.selectable_label(false, msg.ts_str.as_ref());

                        let _ = ui.selectable_label(
                            false,
                            egui::RichText::new(msg.port_nb.as_char())
                                .color(egui::Color32::WHITE)
                                .background_color(row_color),
                        );

                        let repetitions: egui::WidgetText = if msg.repetitions == 1 {
                            "".into()
                        } else if msg.repetitions > MAX_REPETITIONS {
                            MAX_REPETITIONS_EXCEEDED.into()
                        } else {
                            format!("x{}", msg.repetitions).into()
                        };
                        let _ = ui.selectable_label(false, repetitions);

                        let msg_txt =
                            egui::RichText::new(msg.res_str.as_ref()).color(egui::Color32::WHITE);
                        let msg_txt = if msg.is_err {
                            msg_txt.background_color(egui::Color32::DARK_RED)
                        } else {
                            msg_txt.background_color(row_color)
                        };
                        let _ = ui.selectable_label(false, msg_txt);
                        ui.end_row();
                    }
                });

                if self.follows_cursor {
                    ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                }
            })
        });
    }

    pub fn setup(&mut self, storage: Option<&dyn epi::Storage>) {
        if let Some(storage) = storage {
            if let Some(msg_list_dir) = storage.get_string(STORAGE_MSG_LIST_DIR) {
                *self.msg_list_dir.lock().unwrap() = msg_list_dir.into();
            }
        }
    }

    pub fn save(&mut self, storage: &mut dyn epi::Storage) {
        storage.set_string(
            STORAGE_MSG_LIST_DIR,
            self.msg_list_dir.lock().unwrap().display().to_string(),
        );
    }
}

impl MsgListWidget {
    #[must_use]
    pub fn push(&mut self, msg: midi::msg::Result) -> Status {
        let mut status = Status::Unchanged;

        match self.list.last_mut() {
            Some(last) if last.as_ref() == &msg => {
                if last.repetitions < MAX_REPETITIONS {
                    Arc::make_mut(last).repetitions += 1;
                    status.updated();
                }
            }
            _ => {
                let parse_res: MsgParseResult = msg.into();
                self.list.push(parse_res.into());
                status.updated();
            }
        }

        status
    }

    #[cfg(feature = "save")]
    fn save_list(&self) {
        let err_tx = self.err_tx.clone();
        let msg_list = self.list.clone();
        let msg_list_dir = self.msg_list_dir.clone();
        std::thread::spawn(move || {
            use std::fs;

            let file_path = rfd::FileDialog::new()
                .add_filter("Rusty Object Notation (ron)", &["ron"])
                .set_directory(&*msg_list_dir.lock().unwrap().clone())
                .set_file_name("midi_exchg.ron")
                .save_file();

            if let Some(file_path) = file_path {
                match fs::File::create(&file_path) {
                    Ok(file) => {
                        use std::io::{self, Write};

                        let config = ron::ser::PrettyConfig::new();
                        let new_line = config.new_line.clone();
                        // Custom config to keep message fields on a single line
                        // while using spaces between the fields and items.
                        let config = config.new_line(" ".into()).indentor("".into());

                        let mut writer = io::BufWriter::new(file);
                        for msg in msg_list {
                            let config_cl = config.clone();
                            ron::ser::to_writer_pretty(&mut writer, &msg, config_cl).unwrap();
                            writer.write_all(new_line.as_bytes()).unwrap();
                        }

                        *msg_list_dir.lock().unwrap() = file_path
                            .parent()
                            .map_or_else(|| ".".into(), ToOwned::to_owned);
                        log::debug!("Saved Midi messages to: {}", file_path.display());
                    }
                    Err(err) => {
                        log::error!("Couldn't create file {}: {err}", file_path.display());
                        let _ = err_tx.send(Error::Save(err).into());
                    }
                }
            }
        });
    }
}

fn write_chan_voice_msg(
    w: &mut dyn fmt::Write,
    msg: &midi_msg::ChannelVoiceMsg,
) -> std::fmt::Result {
    use midi_msg::ChannelVoiceMsg::*;
    match msg {
        NoteOn {
            ref note,
            ref velocity,
        } => write!(w, "Note {} On vel. {}", note, velocity),
        NoteOff { note, velocity } => write!(w, "Note {} Off vel. {}", note, velocity),
        ControlChange { control } => write!(w, "CC {:?}", control),
        HighResNoteOn { note, velocity } => {
            write!(w, "High Res Note {} On vel. {}", note, velocity)
        }
        HighResNoteOff { note, velocity } => {
            write!(w, "High Res Note {} Off vel. {}", note, velocity)
        }
        PolyPressure { note, pressure } => write!(w, "Poly {} Pressure {}", note, pressure),
        ChannelPressure { pressure } => write!(w, "Channel Pressure {}", pressure),
        ProgramChange { program } => write!(w, "Program Change {}", program),
        PitchBend { bend } => write!(w, "Pitch Bend {}", bend),
    }
}

fn write_poly_mode(w: &mut dyn fmt::Write, pm: &midi_msg::PolyMode) -> std::fmt::Result {
    use midi_msg::PolyMode::*;
    match pm {
        Mono(n_chans) => write!(w, "Mono {} chan(s)", n_chans),
        Poly => w.write_str("Poly"),
    }
}

fn write_chan_mode_msg(w: &mut dyn fmt::Write, msg: &midi_msg::ChannelModeMsg) -> std::fmt::Result {
    use midi_msg::ChannelModeMsg::*;
    match msg {
        AllSoundOff => w.write_str("All Sound Off"),
        AllNotesOff => w.write_str("All Notes Off"),
        ResetAllControllers => w.write_str("Reset All Controllers"),
        OmniMode(om) => write!(w, "Onmi Mode {}", om),
        PolyMode(pm) => {
            w.write_str("Poly Mode ")?;
            write_poly_mode(w, pm)
        }
        LocalControl(lc) => write!(w, "Local Control {}", lc),
    }
}

fn write_time_code_type(w: &mut dyn fmt::Write, tct: &midi_msg::TimeCodeType) -> std::fmt::Result {
    use midi_msg::TimeCodeType::*;
    w.write_str(match tct {
        FPS24 => "24 FPS",
        FPS25 => "25 FPS",
        DF30 => "30 FPS D.F.",
        NDF30 => "30 FPS nD.F.",
    })
}

fn write_time_code(w: &mut dyn fmt::Write, tc: &midi_msg::TimeCode) -> std::fmt::Result {
    write!(
        w,
        "{} frame(s) {}:{}:{} ",
        tc.frames, tc.hours, tc.minutes, tc.seconds,
    )?;
    write_time_code_type(w, &tc.code_type)
}

fn write_sys_com_msg(w: &mut dyn fmt::Write, msg: &midi_msg::SystemCommonMsg) -> std::fmt::Result {
    use midi_msg::SystemCommonMsg::*;
    match msg {
        TimeCodeQuarterFrame1(tc) => {
            w.write_str("Time Code ¼ Frame 1 ")?;
            write_time_code(w, tc)
        }
        TimeCodeQuarterFrame2(tc) => {
            w.write_str("Time Code ¼ Frame 2 ")?;
            write_time_code(w, tc)
        }
        TimeCodeQuarterFrame3(tc) => {
            w.write_str("Time Code ¼ Frame 3 ")?;
            write_time_code(w, tc)
        }
        TimeCodeQuarterFrame4(tc) => {
            w.write_str("Time Code ¼ Frame 4 ")?;
            write_time_code(w, tc)
        }
        TimeCodeQuarterFrame5(tc) => {
            w.write_str("Time Code ¼ Frame 5 ")?;
            write_time_code(w, tc)
        }
        TimeCodeQuarterFrame6(tc) => {
            w.write_str("Time Code ¼ Frame 6 ")?;
            write_time_code(w, tc)
        }
        TimeCodeQuarterFrame7(tc) => {
            w.write_str("Time Code ¼ Frame 7 ")?;
            write_time_code(w, tc)
        }
        TimeCodeQuarterFrame8(tc) => {
            w.write_str("Time Code ¼ Frame 8 ")?;
            write_time_code(w, tc)
        }
        SongPosition(pos) => write!(w, "Song Pos. {}", pos),
        SongSelect(sel) => write!(w, "Song Sel. {}", sel),
        TuneRequest => write!(w, "Tune Req."),
    }
}

fn write_sys_rt_msg(w: &mut dyn fmt::Write, msg: &midi_msg::SystemRealTimeMsg) -> std::fmt::Result {
    use midi_msg::SystemRealTimeMsg::*;
    w.write_str(match msg {
        TimingClock => "Timing Clock",
        Start => "Start",
        Continue => "Continue",
        Stop => "Stop",
        ActiveSensing => "Active Sensing",
        SystemReset => "System Reset",
    })
}

fn write_universal_rt_msg(
    w: &mut dyn fmt::Write,
    msg: &midi_msg::UniversalRealTimeMsg,
) -> std::fmt::Result {
    use midi_msg::UniversalRealTimeMsg::*;
    match msg {
        TimeCodeFull(tc) => {
            write!(w, "Full Time Code ")?;
            write_time_code(w, tc)
        }
        TimeCodeUserBits(user_bits) => write!(w, "Time Code {:?}", user_bits),
        ShowControl(show_ctrl) => write!(w, "Show Ctrl {:?}", show_ctrl),
        TimeSignature(t_sign) => write!(w, "Time Sign. {:?}", t_sign),
        TimeSignatureDelayed(t_sign) => write!(w, "Time Sign. delayed {:?}", t_sign),
        MasterVolume(val) => write!(w, "Master Vol. {}", val),
        MasterBalance(val) => write!(w, "Master Balance {}", val),
        MasterFineTuning(val) => write!(w, "Master fine Tuning {}", val),
        MasterCoarseTuning(val) => write!(w, "Master coarse Tuning {}", val),
        other => write!(w, "{:?}", other),
    }
}

fn write_sysex_msg(w: &mut dyn fmt::Write, msg: &midi_msg::SystemExclusiveMsg) -> std::fmt::Result {
    use midi_msg::SystemExclusiveMsg::*;
    match msg {
        Commercial { id, data } => write!(w, "{:?} {:?}", id, data),
        NonCommercial { data } => write!(w, "Non-com. {:?}", data),
        UniversalRealTime { device, msg } => {
            write!(w, "{:?} ", device)?;
            write_universal_rt_msg(w, msg)
        }
        UniversalNonRealTime { device, msg } => write!(w, "{:?} {:?}", device, msg),
    }
}

fn write_midi_msg(w: &mut dyn fmt::Write, msg: &midi_msg::MidiMsg) -> std::fmt::Result {
    use midi_msg::MidiMsg::*;
    match msg {
        ChannelVoice { channel, msg } => {
            write!(w, "{:?} Voice ", channel)?;
            write_chan_voice_msg(w, msg)
        }
        RunningChannelVoice { channel, msg } => {
            write!(w, "{:?} Voice (running) ", channel)?;
            write_chan_voice_msg(w, msg)
        }
        ChannelMode { channel, msg } => {
            write!(w, "{:?} Mode ", channel)?;
            write_chan_mode_msg(w, msg)
        }
        RunningChannelMode { channel, msg } => {
            write!(w, "{:?} Mode (running) ", channel)?;
            write_chan_mode_msg(w, msg)
        }
        SystemCommon { msg } => {
            w.write_str("Sys. Com. ")?;
            write_sys_com_msg(w, msg)
        }
        SystemRealTime { msg } => {
            w.write_str("Sys. RT ")?;
            write_sys_rt_msg(w, msg)
        }
        SystemExclusive { msg } => {
            w.write_str("Sys. Ex. ")?;
            write_sysex_msg(w, msg)
        }
    }
}
