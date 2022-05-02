use crossbeam_channel as channel;
use eframe::{self, egui};
use std::sync::{Arc, Mutex};

use super::{controller, Dispatcher};
use crate::midi;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}", .0)]
    Port(#[from] midi::port::Error),

    #[error("{}", .0)]
    Midi(#[from] super::port::Error),

    #[error("Failed to parse MIDI Message")]
    Parse(#[from] crate::midi::msg::Error),

    #[error("{}", .0)]
    MsgList(#[from] super::msg_list::Error),
}

pub enum Request {
    Connect((midi::PortNb, Arc<str>)),
    Disconnect(midi::PortNb),
    RefreshPorts,
    Shutdown,
}

pub struct App {
    msg_list_panel: Arc<Mutex<super::MsgListPanel>>,
    req_tx: channel::Sender<Request>,
    err_rx: channel::Receiver<Error>,
    ports_panel: Arc<Mutex<super::PortsPanel>>,
    last_err: Option<Error>,
    controller_thread: Option<std::thread::JoinHandle<()>>,
}

impl App {
    pub fn new(client_name: &str, cc: &eframe::CreationContext) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let (err_tx, err_rx) = channel::unbounded();
        let (req_tx, req_rx) = channel::unbounded();

        let ports_panel = Arc::new(Mutex::new(super::PortsPanel::default()));
        let msg_list_panel = Arc::new(Mutex::new(super::MsgListPanel::new(err_tx.clone(), cc)));

        let controller_thread = controller::Spawner {
            req_rx,
            err_tx,
            msg_list_panel: msg_list_panel.clone(),
            client_name: Arc::from(client_name),
            ports_panel: ports_panel.clone(),
            egui_ctx: cc.egui_ctx.clone(),
        }
        .spawn();

        let mut this = Self {
            msg_list_panel,
            req_tx,
            err_rx,
            ports_panel,
            last_err: None,
            controller_thread: Some(controller_thread),
        };

        for evt in super::PortsPanel::setup(cc.storage) {
            Dispatcher::<super::PortsPanel>::handle(&mut this, Some(evt));
        }

        this
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top-area").show(ctx, |ui| {
            ui.add_space(10f32);
            ui.heading("MIDI Sniffer");
            ui.add_space(10f32);
            ui.horizontal(|ui| {
                use crate::midi::PortNb;

                let resp1 = self.ports_panel.lock().unwrap().show(PortNb::One, ui);
                let resp2 = self.ports_panel.lock().unwrap().show(PortNb::Two, ui);

                Dispatcher::<super::PortsPanel>::handle(self, resp1.or(resp2));
            });
            ui.add_space(5f32);
        });

        egui::TopBottomPanel::bottom("status-area").show(ctx, |ui| {
            self.pop_err();
            if let Some(ref err) = self.last_err {
                ui.add_space(5f32);
                let text = egui::RichText::new(err.to_string())
                    .color(egui::Color32::WHITE)
                    .background_color(egui::Color32::DARK_RED);
                ui.group(|ui| {
                    use egui::Widget;
                    let label = egui::Label::new(text).sense(egui::Sense::click());
                    if label.ui(ui).clicked() {
                        self.clear_last_err();
                    }
                });
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.msg_list_panel.lock().unwrap().show(ui);
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.ports_panel.lock().unwrap().save(storage);
        self.msg_list_panel.lock().unwrap().save(storage);
        self.clear_last_err();
    }

    fn persist_egui_memory(&self) -> bool {
        // Don't persist otherwise this keeps columns and row sizes.
        false
    }

    fn on_exit(&mut self, _gl: &eframe::glow::Context) {
        log::info!("Shutting down");
        self.shutdown();
    }
}

impl App {
    pub fn shutdown(&mut self) {
        if let Some(controller_thread) = self.controller_thread.take() {
            if let Err(err) = self.req_tx.send(Request::Shutdown) {
                log::error!("Sniffer couldn't request shutdown: {}", err);
            } else {
                let _ = controller_thread.join();
            }
        }
    }

    pub fn send_req(&mut self, req: Request) {
        self.req_tx.send(req).unwrap();
    }

    pub fn clear_last_err(&mut self) {
        self.last_err = None;
    }

    fn pop_err(&mut self) {
        match self.err_rx.try_recv() {
            Err(channel::TryRecvError::Empty) => (),
            Ok(err) => self.last_err = Some(err),
            Err(err) => panic!("{}", err),
        }
    }
}
