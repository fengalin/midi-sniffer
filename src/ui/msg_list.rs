use eframe::egui;
use std::fmt::Write;

const MAX_REPETITIONS: u8 = 99;
const MAX_REPETITIONS_EXCEEDED: &str = ">99";

pub struct MsgParseResult {
    ts: String,
    repetitions: u8,
    displayable: String,
    res: Result<midi_msg::MidiMsg, super::sniffer::Error>,
}

impl PartialEq<super::sniffer::MidiMsgParseResult> for MsgParseResult {
    fn eq(&self, other: &super::sniffer::MidiMsgParseResult) -> bool {
        match (&self.res, other) {
            (Ok(s), Ok(o)) => s.eq(&o.msg),
            (Err(_), Err((_, oerr))) if self.displayable == format!("{oerr}") => true,
            _ => false,
        }
    }
}

impl From<super::sniffer::MidiMsgParseResult> for MsgParseResult {
    fn from(res: super::sniffer::MidiMsgParseResult) -> Self {
        match res {
            Ok(msg) => {
                let mut displayable = String::new();
                write_midi_msg(&mut displayable, &msg.msg).unwrap();

                Self {
                    ts: format!("{}", msg.ts),
                    repetitions: 1,
                    displayable,
                    res: Ok(msg.msg),
                }
            }
            Err((ts, err)) => Self {
                ts: format!("{ts}"),
                repetitions: 1,
                displayable: format!("{err}"),
                res: Err(err),
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

#[derive(Default)]
pub struct MsgListWidget {
    pub list: Vec<MsgParseResult>,
}

impl MsgListWidget {
    pub fn show(&self, ui: &mut egui::Ui) {
        // FIXME find a way to auto scroll
        egui::ScrollArea::both().show(ui, |ui| {
            egui::Grid::new("Msg List").show(ui, |ui| {
                ui.label("Timestamp");
                ui.label("Rep.");
                ui.label("Message");
                ui.end_row();

                ui.separator();
                ui.separator();
                ui.separator();
                ui.end_row();

                for msg in self.list.iter() {
                    let _ = ui.selectable_label(false, &msg.ts);

                    if msg.repetitions == 1 {
                        let _ = ui.selectable_label(false, "");
                    } else if msg.repetitions > MAX_REPETITIONS {
                        let _ = ui.selectable_label(false, MAX_REPETITIONS_EXCEEDED);
                    } else {
                        let _ = ui.selectable_label(false, &format!("x{}", msg.repetitions));
                    };

                    if msg.res.is_err() {
                        let _ = ui.colored_label(egui::Color32::RED, &msg.displayable);
                    } else {
                        let _ = ui.selectable_label(false, &msg.displayable);
                    }
                    ui.end_row();
                }
            });
        });
    }

    #[must_use]
    pub fn extend(
        &mut self,
        msg_iter: impl Iterator<Item = super::sniffer::MidiMsgParseResult>,
    ) -> Status {
        let mut status = Status::Unchanged;

        for msg in msg_iter {
            match self.list.last_mut() {
                Some(last) if last == &msg => {
                    if last.repetitions < MAX_REPETITIONS {
                        last.repetitions += 1;
                        status.updated();
                    }
                }
                _ => {
                    self.list.push(msg.into());
                    status.updated();
                }
            }
        }

        status
    }
}

fn write_chan_voice_msg(w: &mut dyn Write, msg: &midi_msg::ChannelVoiceMsg) -> std::fmt::Result {
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

fn write_poly_mode(w: &mut dyn Write, pm: &midi_msg::PolyMode) -> std::fmt::Result {
    use midi_msg::PolyMode::*;
    match pm {
        Mono(n_chans) => write!(w, "Mono {} chan(s)", n_chans),
        Poly => w.write_str("Poly"),
    }
}

fn write_chan_mode_msg(w: &mut dyn Write, msg: &midi_msg::ChannelModeMsg) -> std::fmt::Result {
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

fn write_time_code_type(w: &mut dyn Write, tct: &midi_msg::TimeCodeType) -> std::fmt::Result {
    use midi_msg::TimeCodeType::*;
    w.write_str(match tct {
        FPS24 => "24 FPS",
        FPS25 => "25 FPS",
        DF30 => "30 FPS D.F.",
        NDF30 => "30 FPS nD.F.",
    })
}

fn write_time_code(w: &mut dyn Write, tc: &midi_msg::TimeCode) -> std::fmt::Result {
    write!(
        w,
        "{} frame(s) {}:{}:{} ",
        tc.frames, tc.hours, tc.minutes, tc.seconds,
    )?;
    write_time_code_type(w, &tc.code_type)
}

fn write_sys_com_msg(w: &mut dyn Write, msg: &midi_msg::SystemCommonMsg) -> std::fmt::Result {
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

fn write_sys_rt_msg(w: &mut dyn Write, msg: &midi_msg::SystemRealTimeMsg) -> std::fmt::Result {
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
    w: &mut dyn Write,
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

fn write_sysex_msg(w: &mut dyn Write, msg: &midi_msg::SystemExclusiveMsg) -> std::fmt::Result {
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

fn write_midi_msg(w: &mut dyn Write, msg: &midi_msg::MidiMsg) -> std::fmt::Result {
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
