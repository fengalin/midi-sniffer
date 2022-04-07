use crossbeam_channel as channel;
use eframe::epi;
use std::{
    ops::ControlFlow,
    sync::{Arc, Mutex},
};

use super::app;
use crate::midi;

pub struct Spawner {
    pub req_rx: channel::Receiver<app::Request>,
    pub err_tx: channel::Sender<app::Error>,
    pub msg_list_panel: Arc<Mutex<super::MsgListPanel>>,
    pub client_name: Arc<str>,
    pub ports_panel: Arc<Mutex<super::PortsPanel>>,
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
            );
        })
    }
}

struct Controller {
    err_tx: channel::Sender<app::Error>,

    midi_tx: channel::Sender<midi::msg::Origin>,
    msg_list_panel: Arc<Mutex<super::MsgListPanel>>,

    midi_ports: midi::Ports,
    ports_panel: Arc<Mutex<super::PortsPanel>>,

    must_repaint: bool,
    frame: Option<epi::Frame>,
}

impl Controller {
    fn run(
        req_rx: channel::Receiver<app::Request>,
        err_tx: channel::Sender<app::Error>,
        msg_list_panel: Arc<Mutex<super::MsgListPanel>>,
        client_name: Arc<str>,
        ports_panel: Arc<Mutex<super::PortsPanel>>,
    ) -> Result<(), ()> {
        let midi_ports = midi::Ports::try_new(client_name).map_err(|err| {
            log::error!("Error creating Controller: {}", err);
            let _ = err_tx.send(err.into());
        })?;

        let (midi_tx, midi_rx) = channel::unbounded();

        Self {
            err_tx,

            midi_tx,
            msg_list_panel,

            midi_ports,
            ports_panel,

            must_repaint: false,
            frame: None,
        }
        .run_loop(req_rx, midi_rx);

        Ok(())
    }

    fn handle(&mut self, request: app::Request) -> Result<ControlFlow<(), ()>, app::Error> {
        use app::Request::*;
        match request {
            Connect((port_nb, port_name)) => self.connect(port_nb, port_name)?,
            Disconnect(port_nb) => self.disconnect(port_nb)?,
            RefreshPorts => self.refresh_ports()?,
            Shutdown => return Ok(ControlFlow::Break(())),
            HaveFrame(egui_frame) => {
                self.frame = Some(egui_frame);
            }
        }

        Ok(ControlFlow::Continue(()))
    }

    fn connect(&mut self, port_nb: midi::PortNb, port_name: Arc<str>) -> Result<(), app::Error> {
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

    fn disconnect(&mut self, port_nb: midi::PortNb) -> Result<(), app::Error> {
        self.midi_ports.disconnect(port_nb)?;
        self.refresh_ports()?;

        Ok(())
    }

    fn refresh_ports(&mut self) -> Result<(), app::Error> {
        self.midi_ports.refresh()?;
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
                if let Some(ref frame) = self.frame {
                    frame.request_repaint();
                }
                self.must_repaint = false;
            }
        }

        log::debug!("Shutting down Sniffer Controller loop");
    }
}
