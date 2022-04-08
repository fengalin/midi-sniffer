use crossbeam_channel as channel;
use eframe::{egui, epi};
use std::{
    fmt,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::{
    bytes,
    midi::{self, PortNb},
};

const MAX_REPETITIONS: u8 = 99;
const MAX_REPETITIONS_EXCEEDED: &str = ">99";
const STORAGE_MSG_LIST_DIR: &str = "msg_list_dir";
const STORAGE_MSG_LIST_DISPLAY_PARSED: &str = "msg_list_must_display_parsed";
const STORAGE_MSG_LIST_DISPLAY_RAW: &str = "msg_list_must_display_raw";

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
    ts_str: String,

    #[cfg_attr(feature = "save", serde(rename = "port"))]
    port_nb: PortNb,

    repetitions: u8,

    is_err: bool,

    #[cfg_attr(feature = "save", serde(rename = "parsed"))]
    parsed_res_str: String,

    #[cfg_attr(feature = "save", serde(skip))]
    raw_str: String,

    #[cfg_attr(feature = "save", serde(rename = "raw"))]
    raw: Buffer,
}

#[derive(Clone, Debug, PartialEq)]
struct Buffer(Arc<[u8]>);

impl Buffer {
    pub fn display(&self) -> bytes::Displayable {
        bytes::Displayable::from(self.0.as_ref())
    }
}

impl PartialEq<[u8]> for Buffer {
    fn eq(&self, other: &[u8]) -> bool {
        self.0.as_ref().eq(other)
    }
}

impl From<Arc<[u8]>> for Buffer {
    fn from(buf: Arc<[u8]>) -> Self {
        Self(buf)
    }
}

/// Serialize as hex printable values.
#[cfg(feature = "save")]
impl<'a> serde::Serialize for Buffer {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("(hex) {}", self.display()))
    }
}

impl PartialEq<midi::msg::Result> for MsgParseResult {
    fn eq(&self, other: &midi::msg::Result) -> bool {
        let other_origin = match other {
            Ok(ok) => &ok.origin,
            Err(err) => &err.origin,
        };
        self.port_nb == other_origin.port_nb && self.raw == *other_origin.buffer
    }
}

impl From<midi::msg::Result> for MsgParseResult {
    fn from(res: midi::msg::Result) -> Self {
        match res {
            Ok(ok) => {
                let mut parsed_str = String::new();
                write_midi_msg(&mut parsed_str, &ok.msg).unwrap();

                let raw: Buffer = ok.origin.buffer.into();

                Self {
                    ts_str: format!("{}", ok.origin.ts),
                    port_nb: ok.origin.port_nb,
                    repetitions: 1,
                    parsed_res_str: parsed_str,
                    raw_str: format!("{}", raw.display()),
                    raw,
                    is_err: false,
                }
            }
            Err(err) => {
                let raw: Buffer = err.origin.buffer.into();

                Self {
                    ts_str: format!("{}", err.origin.ts),
                    port_nb: err.origin.port_nb,
                    repetitions: 1,
                    parsed_res_str: format!("{}", err.err),
                    raw_str: format!("{}", raw.display()),
                    raw,
                    is_err: true,
                }
            }
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

pub struct MsgListPanel {
    pub list: Vec<Arc<MsgParseResult>>,
    follows_cursor: bool,
    must_display_parsed: bool,
    must_display_raw: bool,
    err_tx: channel::Sender<super::app::Error>,
    msg_list_dir: Arc<Mutex<PathBuf>>,
}

impl MsgListPanel {
    pub fn new(err_tx: channel::Sender<super::app::Error>) -> Self {
        Self {
            list: Vec::new(),
            follows_cursor: true,
            must_display_parsed: true,
            must_display_raw: false,
            err_tx,
            msg_list_dir: Arc::new(Mutex::new(PathBuf::from("."))),
        }
    }
}

impl MsgListPanel {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.follows_cursor, "Follow");
                ui.add_enabled_ui(!self.list.is_empty(), |ui| {
                    if ui.button("Clear").clicked() {
                        self.list.clear();
                    }

                    ui.separator();

                    ui.checkbox(&mut self.must_display_parsed, "Parsed");
                    ui.checkbox(&mut self.must_display_raw, "Raw");

                    ui.separator();

                    #[cfg(feature = "save")]
                    if ui.button("Save").clicked() {
                        self.save_list();
                    }
                });
            });

            ui.separator();
            egui::ScrollArea::both().show(ui, |ui| {
                // Adapt grid id otherwise column sizes are kept
                // between refresh, regardless of the columns added.
                let mut grid_id = String::from("MsgLst");

                let mut num_columns = 3;
                if self.must_display_parsed {
                    num_columns += 1;
                    grid_id.push('P');
                }
                if self.must_display_raw {
                    num_columns += 1;
                    grid_id.push('R');
                }

                egui::Grid::new(grid_id)
                    .num_columns(num_columns)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Timestamp");
                        ui.label("Port");
                        ui.label("Rep.");
                        if self.must_display_parsed {
                            ui.label("Parsed msg");
                        }
                        if self.must_display_raw {
                            ui.label("Raw msg (hex)");
                        }
                        ui.end_row();

                        for _ in 0..num_columns {
                            ui.separator();
                        }
                        ui.end_row();

                        for msg in self.list.iter() {
                            let row_color = match msg.port_nb {
                                midi::PortNb::One => egui::Color32::from_rgb(0, 0, 0x64),
                                midi::PortNb::Two => egui::Color32::from_rgb(0, 0x48, 0),
                            };

                            let _ = ui.selectable_label(false, &msg.ts_str);

                            let _ = ui.selectable_label(
                                false,
                                egui::RichText::new(msg.port_nb.as_char())
                                    .color(egui::Color32::WHITE)
                                    .background_color(row_color),
                            );

                            let repetitions: egui::WidgetText = if msg.repetitions == 1 {
                                "".into()
                            } else if msg.repetitions <= MAX_REPETITIONS {
                                format!("x{}", msg.repetitions).into()
                            } else {
                                MAX_REPETITIONS_EXCEEDED.into()
                            };
                            let _ = ui.selectable_label(false, repetitions);

                            if self.must_display_parsed {
                                let msg_txt = egui::RichText::new(&msg.parsed_res_str)
                                    .color(egui::Color32::WHITE);
                                let msg_txt = if msg.is_err {
                                    msg_txt.background_color(egui::Color32::DARK_RED)
                                } else {
                                    msg_txt.background_color(row_color)
                                };
                                let _ = ui.selectable_label(false, msg_txt);
                            }

                            if self.must_display_raw {
                                let raw_txt = egui::RichText::new(&msg.raw_str)
                                    .color(egui::Color32::WHITE)
                                    .background_color(row_color);
                                let _ = ui.selectable_label(false, raw_txt);
                            }

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
            if let Some(display_parsed) = storage.get_string(STORAGE_MSG_LIST_DISPLAY_PARSED) {
                self.must_display_parsed = display_parsed == "true";
            }
            if let Some(display_raw) = storage.get_string(STORAGE_MSG_LIST_DISPLAY_RAW) {
                self.must_display_raw = display_raw == "true";
            }
        }
    }

    pub fn save(&mut self, storage: &mut dyn epi::Storage) {
        storage.set_string(
            STORAGE_MSG_LIST_DIR,
            self.msg_list_dir.lock().unwrap().display().to_string(),
        );

        storage.set_string(
            STORAGE_MSG_LIST_DISPLAY_PARSED,
            format!("{}", self.must_display_parsed),
        );

        storage.set_string(
            STORAGE_MSG_LIST_DISPLAY_RAW,
            format!("{}", self.must_display_raw),
        );
    }
}

impl MsgListPanel {
    #[must_use]
    pub fn push(&mut self, msg: midi::msg::Result) -> Status {
        let mut status = Status::Unchanged;

        match self.list.last_mut() {
            Some(last) if last.as_ref() == &msg => {
                if last.repetitions <= MAX_REPETITIONS {
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

fn write_cc_msg(w: &mut dyn fmt::Write, msg: &midi_msg::ControlChange) -> std::fmt::Result {
    use midi_msg::ControlChange::*;
    match msg {
        BankSelect(val) => write!(w, "Bank Select {val}"),
        ModWheel(val) => write!(w, "Mod Wheel {val}"),
        Breath(val) => write!(w, "Breath {val}"),
        Undefined { control, value } => {
            write!(w, "Undef ctrl {control} val {value}")
        }
        UndefinedHighRes {
            control1,
            control2,
            value,
        } => write!(
            w,
            "Undef High Res ctrl ({control1}, {control2}) val {value}"
        ),
        Foot(val) => write!(w, "Foot {val}"),
        Portamento(val) => write!(w, "Portamento {val}"),
        Volume(val) => write!(w, "Volume {val}"),
        Balance(val) => write!(w, "Balance {val}"),
        Pan(val) => write!(w, "Pan {val}"),
        Expression(val) => write!(w, "Expression {val}"),
        Effect1(val) => write!(w, "Effect 1 {val}"),
        Effect2(val) => write!(w, "Effect 2 {val}"),
        GeneralPurpose1(val) => write!(w, "General Purpose 1 {val}"),
        GeneralPurpose2(val) => write!(w, "General Purpose 2 {val}"),
        GeneralPurpose3(val) => write!(w, "General Purpose 3 {val}"),
        GeneralPurpose4(val) => write!(w, "General Purpose 4 {val}"),
        GeneralPurpose5(val) => write!(w, "General Purpose 5 {val}"),
        GeneralPurpose6(val) => write!(w, "General Purpose 6 {val}"),
        GeneralPurpose7(val) => write!(w, "General Purpose 7 {val}"),
        GeneralPurpose8(val) => write!(w, "General Purpose 8 {val}"),
        Hold(val) => write!(w, "Hold {val}"),
        Hold2(val) => write!(w, "Hold 2 {val}"),
        TogglePortamento(val) => write!(w, "Toggle Portamento {val}"),
        Sostenuto(val) => write!(w, "Sostenuto {val}"),
        SoftPedal(val) => write!(w, "Soft Pedal {val}"),
        ToggleLegato(val) => write!(w, "Toggle Legato {val}"),
        SoundVariation(val) => write!(w, "Sound Variation {val}"),
        Timbre(val) => write!(w, "Timbre {val}"),
        ReleaseTime(val) => write!(w, "Release Time {val}"),
        AttackTime(val) => write!(w, "Attack Time {val}"),
        Brightness(val) => write!(w, "Brightness {val}"),
        DecayTime(val) => write!(w, "Decay Time {val}"),
        VibratoRate(val) => write!(w, "Vibrato Rate {val}"),
        VibratoDepth(val) => write!(w, "Vibrato Depth {val}"),
        VibratoDelay(val) => write!(w, "Vibrato Delay {val}"),
        SoundControl1(val) => write!(w, "Sound Ctrl 1 {val}"),
        SoundControl2(val) => write!(w, "Sound Ctrl 2 {val}"),
        SoundControl3(val) => write!(w, "Sound Ctrl 3 {val}"),
        SoundControl4(val) => write!(w, "Sound Ctrl 4 {val}"),
        SoundControl5(val) => write!(w, "Sound Ctrl 5 {val}"),
        SoundControl6(val) => write!(w, "Sound Ctrl 6 {val}"),
        SoundControl7(val) => write!(w, "Sound Ctrl 7 {val}"),
        SoundControl8(val) => write!(w, "Sound Ctrl 8 {val}"),
        SoundControl9(val) => write!(w, "Sound Ctrl 9 {val}"),
        SoundControl10(val) => write!(w, "Sound Ctrl 10 {val}"),
        HighResVelocity(val) => write!(w, "High Res Velocity {val}"),
        PortamentoControl(val) => write!(w, "Portamento Control {val}"),
        Effects1Depth(val) => write!(w, "Effects 1 Depth {val}"),
        Effects2Depth(val) => write!(w, "Effects 2 Depth {val}"),
        Effects3Depth(val) => write!(w, "Effects 3 Depth {val}"),
        Effects4Depth(val) => write!(w, "Effects 4 Depth {val}"),
        Effects5Depth(val) => write!(w, "Effects 5 Depth {val}"),
        ReverbSendLevel(val) => write!(w, "Reverb Send Level {val}"),
        TremoloDepth(val) => write!(w, "Tremolo Depth {val}"),
        ChorusSendLevel(val) => write!(w, "Chorus Send Level {val}"),
        CelesteDepth(val) => write!(w, "Celeste Depth {val}"),
        PhaserDepth(val) => write!(w, "Phaser Depth {val}"),
        Parameter(param) => write!(w, "Parameter {param:?}"),
        DataEntry(val) => write!(w, "Data Entry w{val:04x}"),
        DataEntry2(val1, val2) => write!(w, "Data Entry 2 x{val1:02x} x{val2:02x}"),
        DataIncrement(val) => write!(w, "Data Inc {val}"),
        DataDecrement(val) => write!(w, "Data Dec {val}"),
    }
}

fn write_chan_voice_msg(
    w: &mut dyn fmt::Write,
    msg: &midi_msg::ChannelVoiceMsg,
) -> std::fmt::Result {
    use midi_msg::ChannelVoiceMsg::*;
    match msg {
        NoteOn { note, velocity } => write!(w, "Note {note} On vel. {velocity}"),
        NoteOff { note, velocity } => write!(w, "Note {note} Off vel. {velocity}"),
        ControlChange { control } => {
            write!(w, "CC ")?;
            write_cc_msg(w, control)
        }
        HighResNoteOn { note, velocity } => {
            write!(w, "High Res Note {note} On vel. {velocity}")
        }
        HighResNoteOff { note, velocity } => {
            write!(w, "High Res Note {note} Off vel. {velocity}")
        }
        PolyPressure { note, pressure } => {
            write!(w, "Poly Note {note} Pressure {pressure}")
        }
        ChannelPressure { pressure } => write!(w, "Channel Pressure {pressure}"),
        ProgramChange { program } => write!(w, "Program Change {program}"),
        PitchBend { bend } => write!(w, "Pitch Bend {bend}"),
    }
}

fn write_poly_mode(w: &mut dyn fmt::Write, pm: &midi_msg::PolyMode) -> std::fmt::Result {
    use midi_msg::PolyMode::*;
    match pm {
        Mono(n_chans) => write!(w, "Mono {n_chans} chan(s)"),
        Poly => w.write_str("Poly"),
    }
}

fn write_chan_mode_msg(w: &mut dyn fmt::Write, msg: &midi_msg::ChannelModeMsg) -> std::fmt::Result {
    use midi_msg::ChannelModeMsg::*;
    match msg {
        AllSoundOff => w.write_str("All Sound Off"),
        AllNotesOff => w.write_str("All Notes Off"),
        ResetAllControllers => w.write_str("Reset All Controllers"),
        OmniMode(om) => write!(w, "Onmi Mode {om}"),
        PolyMode(pm) => {
            w.write_str("Poly Mode ")?;
            write_poly_mode(w, pm)
        }
        LocalControl(lc) => write!(w, "Local Control {lc}"),
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
        SongPosition(pos) => write!(w, "Song Pos. {pos}"),
        SongSelect(sel) => write!(w, "Song Sel. {sel}"),
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
        TimeCodeUserBits(user_bits) => write!(w, "Time Code {user_bits:?}"),
        ShowControl(show_ctrl) => write!(w, "Show Ctrl {show_ctrl:?}"),
        TimeSignature(t_sign) => write!(w, "Time Sign. {t_sign:?}"),
        TimeSignatureDelayed(t_sign) => write!(w, "Time Sign. delayed {t_sign:?}"),
        MasterVolume(val) => write!(w, "Master Vol. {val}"),
        MasterBalance(val) => write!(w, "Master Balance {val}"),
        MasterFineTuning(val) => write!(w, "Master fine Tuning {val}"),
        MasterCoarseTuning(val) => write!(w, "Master coarse Tuning {val}"),
        other => write!(w, "{:?}", other),
    }
}

fn write_sysex_msg(w: &mut dyn fmt::Write, msg: &midi_msg::SystemExclusiveMsg) -> std::fmt::Result {
    use midi_msg::SystemExclusiveMsg::*;
    match msg {
        Commercial { id, data } => {
            write!(
                w,
                "{id:?} data {}",
                bytes::Displayable::from(data.as_slice())
            )
        }
        NonCommercial { data } => {
            write!(
                w,
                "Non-com. data {}",
                bytes::Displayable::from(data.as_slice())
            )
        }
        UniversalRealTime { device, msg } => {
            write!(w, "UniRT {device:?} ")?;
            write_universal_rt_msg(w, msg)
        }
        UniversalNonRealTime { device, msg } => write!(w, "UniNonRT {device:?} {msg:?}"),
    }
}

fn write_midi_msg(w: &mut dyn fmt::Write, msg: &midi_msg::MidiMsg) -> std::fmt::Result {
    use midi_msg::MidiMsg::*;
    match msg {
        ChannelVoice { channel, msg } => {
            write!(w, "{channel:?} Voice ")?;
            write_chan_voice_msg(w, msg)
        }
        RunningChannelVoice { channel, msg } => {
            write!(w, "{channel:?} Voice (running) ")?;
            write_chan_voice_msg(w, msg)
        }
        ChannelMode { channel, msg } => {
            write!(w, "{channel:?} Mode ")?;
            write_chan_mode_msg(w, msg)
        }
        RunningChannelMode { channel, msg } => {
            write!(w, "{channel:?} Mode (running) ")?;
            write_chan_mode_msg(w, msg)
        }
        SystemCommon { msg } => {
            w.write_str("SysCom ")?;
            write_sys_com_msg(w, msg)
        }
        SystemRealTime { msg } => {
            w.write_str("SysRT ")?;
            write_sys_rt_msg(w, msg)
        }
        SystemExclusive { msg } => {
            w.write_str("SysEx ")?;
            write_sysex_msg(w, msg)
        }
    }
}
