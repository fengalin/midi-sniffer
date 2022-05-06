use anyhow::Context;
use crossbeam_channel as channel;
use eframe::egui;
use std::{
    ops::ControlFlow,
    sync::{Arc, Mutex},
};

use super::app;
use crate::midi;

pub struct Spawner {
    pub req_rx: channel::Receiver<app::Request>,
    pub err_tx: channel::Sender<anyhow::Error>,
    pub msg_list_panel: Arc<Mutex<super::MsgListPanel>>,
    pub client_name: Arc<str>,
    pub ports_panel: Arc<Mutex<super::PortsPanel>>,
    pub egui_ctx: egui::Context,
}

impl Spawner {
    pub fn spawn(self) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let _ = Controller::run(
                self.req_rx,
                self.err_tx,
                self.msg_list_panel,
                self.client_name,
                self.ports_panel,
                self.egui_ctx,
            );
        })
    }
}

struct Controller {
    err_tx: channel::Sender<anyhow::Error>,

    midi_tx: channel::Sender<midi::msg::Origin>,
    msg_list_panel: Arc<Mutex<super::MsgListPanel>>,

    midi_ports: midi::Ports,
    ports_panel: Arc<Mutex<super::PortsPanel>>,

    must_repaint: bool,
    egui_ctx: egui::Context,
}

impl Controller {
    fn run(
        req_rx: channel::Receiver<app::Request>,
        err_tx: channel::Sender<anyhow::Error>,
        msg_list_panel: Arc<Mutex<super::MsgListPanel>>,
        client_name: Arc<str>,
        ports_panel: Arc<Mutex<super::PortsPanel>>,
        egui_ctx: egui::Context,
    ) -> Result<(), ()> {
        let midi_ports = midi::Ports::try_new(client_name)
            .context("Failed to create Controller")
            .map_err(|err| {
                log::error!("{err}");
                let _ = err_tx.send(err);
            })?;

        let (midi_tx, midi_rx) = channel::unbounded();

        Self {
            err_tx,

            midi_tx,
            msg_list_panel,

            midi_ports,
            ports_panel,

            must_repaint: false,
            egui_ctx,
        }
        .run_loop(req_rx, midi_rx);

        Ok(())
    }

    fn handle(&mut self, request: app::Request) -> anyhow::Result<ControlFlow<(), ()>> {
        use app::Request::*;
        match request {
            Connect((port_nb, port_name)) => self.connect(port_nb, port_name)?,
            Disconnect(port_nb) => self.disconnect(port_nb)?,
            RefreshPorts => self.refresh_ports()?,
            Shutdown => return Ok(ControlFlow::Break(())),
        }

        Ok(ControlFlow::Continue(()))
    }

    fn connect(&mut self, port_nb: midi::PortNb, port_name: Arc<str>) -> anyhow::Result<()> {
        let midi_tx = self.midi_tx.clone();
        let callback = move |ts, buf: &[u8]| {
            midi_tx
                .send(midi::msg::Origin::new(ts, port_nb, buf))
                .unwrap();
        };

        self.midi_ports.connect(port_nb, port_name, callback)?;
        self.refresh_ports()?;

        Ok(())
    }

    fn disconnect(&mut self, port_nb: midi::PortNb) -> anyhow::Result<()> {
        self.midi_ports.disconnect(port_nb)?;
        self.refresh_ports()?;

        Ok(())
    }

    fn refresh_ports(&mut self) -> anyhow::Result<()> {
        self.midi_ports
            .refresh()
            .context("Failed to refresh ports")?;
        self.ports_panel.lock().unwrap().update(&self.midi_ports);

        Ok(())
    }

    fn run_loop(
        mut self,
        req_rx: channel::Receiver<app::Request>,
        midi_rx: channel::Receiver<midi::msg::Origin>,
    ) {
        if let Err(err) = self.refresh_ports() {
            let _ = self.err_tx.send(err);
        }

        loop {
            channel::select! {
                recv(req_rx) -> request =>  {
                    match request {
                        Ok(request) => match self.handle(request) {
                            Ok(ControlFlow::Continue(())) => (),
                            Ok(ControlFlow::Break(())) => break,
                            Err(err) => {
                                log::error!("{err}");
                                let _ = self.err_tx.send(err);
                            }
                        }
                        Err(err) => {
                            log::error!("Error UI request channel: {err}");
                            break;
                        }
                    }
                }
                recv(midi_rx) -> midi_msg =>  {
                    match midi_msg {
                        Ok(origin) => {
                            let res = match midi_msg::MidiMsg::from_midi(&origin.buffer) {
                                Ok((msg, _len)) => Ok(midi::Msg { origin, msg }),
                                Err(err) => {
                                    log::error!("Failed to parse Midi buffer: {err}");
                                    Err(midi::msg::Error { origin, err })
                                }
                            };

                            self.must_repaint =
                                { self.msg_list_panel.lock().unwrap().push(res) }.was_updated();
                        }
                        Err(err) => {
                            log::error!("Error MIDI message channel: {err}");
                            break;
                        }
                    }
                }
            }

            if self.must_repaint {
                self.egui_ctx.request_repaint();
                self.must_repaint = false;
            }
        }

        log::debug!("Shutting down Sniffer Controller loop");
    }
}
