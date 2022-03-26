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
    pub msg_list_widget: Arc<Mutex<super::MsgListWidget>>,
    pub client_name: Arc<str>,
    pub ports_widget: Arc<Mutex<super::PortsWidget>>,
}

impl Spawner {
    pub fn spawn(self) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let _ = Controller::run(
                self.req_rx,
                self.err_tx,
                self.msg_list_widget,
                self.client_name,
                self.ports_widget,
            );
        })
    }
}

struct Controller {
    err_tx: channel::Sender<app::Error>,

    msg_tx: channel::Sender<midi::msg::Result>,
    msg_list_widget: Arc<Mutex<super::MsgListWidget>>,

    midi_ports: midi::Ports,
    ports_widget: Arc<Mutex<super::PortsWidget>>,

    must_repaint: bool,
    frame: Option<epi::Frame>,
}

impl Controller {
    fn run(
        req_rx: channel::Receiver<app::Request>,
        err_tx: channel::Sender<app::Error>,
        msg_list_widget: Arc<Mutex<super::MsgListWidget>>,
        client_name: Arc<str>,
        ports_widget: Arc<Mutex<super::PortsWidget>>,
    ) -> Result<(), ()> {
        let midi_ports = midi::Ports::try_new(client_name).map_err(|err| {
            log::error!("Error creating Controller: {}", err);
            let _ = err_tx.send(err.into());
        })?;

        let (msg_tx, msg_rx) = channel::unbounded();

        Self {
            err_tx,

            msg_tx,
            msg_list_widget,

            midi_ports,
            ports_widget,

            must_repaint: false,
            frame: None,
        }
        .run_loop(req_rx, msg_rx);

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
        let msg_tx = self.msg_tx.clone();
        let callback = move |ts, buf: &[u8]| {
            let origin = midi::msg::Origin::new(ts, port_nb, buf);
            match midi_msg::MidiMsg::from_midi(&origin.buffer) {
                Ok((msg, _len)) => {
                    msg_tx.send(Ok(midi::Msg { origin, msg })).unwrap();
                }
                Err(err) => {
                    log::error!("Failed to parse Midi buffer: {}", err);
                    msg_tx.send(Err(midi::msg::Error { origin, err })).unwrap();
                }
            }
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
        self.ports_widget.lock().unwrap().update(&self.midi_ports);

        Ok(())
    }

    fn run_loop(
        mut self,
        req_rx: channel::Receiver<app::Request>,
        msg_rx: channel::Receiver<midi::msg::Result>,
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
                recv(msg_rx) -> msg =>  {
                    match msg {
                        Ok(msg) => {
                            self.must_repaint =
                            { self.msg_list_widget.lock().unwrap().push(msg) }.was_updated();
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
