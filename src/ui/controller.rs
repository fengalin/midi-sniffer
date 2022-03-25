use crossbeam_channel as channel;
use eframe::epi;
use std::{
    ops::ControlFlow,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use super::app;
use crate::midi;

const MSG_POLLING_INTERVAL: Duration = Duration::from_millis(20);
const MSG_LIST_BATCH_SIZE: usize = 5;
const MAX_MSG_BATCHES_PER_UPDATE: usize = 30 / MSG_LIST_BATCH_SIZE;

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
            let _ = Controller::try_new(
                self.req_rx,
                self.err_tx,
                self.msg_list_widget,
                self.client_name,
                self.ports_widget,
            )
            .map(Controller::run);
        })
    }
}

struct Controller {
    req_rx: channel::Receiver<app::Request>,
    err_tx: channel::Sender<app::Error>,

    msg_rx: channel::Receiver<midi::msg::Result>,
    msg_tx: channel::Sender<midi::msg::Result>,
    msg_list_widget: Arc<Mutex<super::MsgListWidget>>,

    midi_ports: midi::Ports,
    ports_widget: Arc<Mutex<super::PortsWidget>>,

    must_repaint: bool,
    frame: Option<epi::Frame>,
}

impl Controller {
    fn try_new(
        req_rx: channel::Receiver<app::Request>,
        err_tx: channel::Sender<app::Error>,
        msg_list_widget: Arc<Mutex<super::MsgListWidget>>,
        client_name: Arc<str>,
        ports_widget: Arc<Mutex<super::PortsWidget>>,
    ) -> Result<Self, ()> {
        let midi_ports = midi::Ports::try_new(client_name).map_err(|err| {
            log::error!("Error creating Controller: {}", err);
            let _ = err_tx.send(err.into());
        })?;

        let (msg_tx, msg_rx) = channel::unbounded();

        Ok(Self {
            req_rx,
            err_tx,

            msg_rx,
            msg_tx,
            msg_list_widget,

            midi_ports,
            ports_widget,

            must_repaint: false,
            frame: None,
        })
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

    fn try_receive_request(&mut self) -> Option<app::Request> {
        let request = self
            .req_rx
            .recv_deadline(Instant::now() + MSG_POLLING_INTERVAL);
        for _nb in 0..MAX_MSG_BATCHES_PER_UPDATE {
            // Update msg list widget with batches of at most
            // MSG_LIST_BATCH_SIZE messages so as not to lock the widget for too long.
            let mut msg_batch_iter = self.msg_rx.try_iter().take(MSG_LIST_BATCH_SIZE).peekable();
            if msg_batch_iter.peek().is_none() {
                break;
            }

            self.must_repaint =
                { self.msg_list_widget.lock().unwrap().extend(msg_batch_iter) }.was_updated();
        }

        match request {
            Ok(request) => Some(request),
            Err(channel::RecvTimeoutError::Timeout) => None,
            Err(err) => panic!("{}", err),
        }
    }

    fn run(mut self) {
        loop {
            if let Err(err) = self.refresh_ports() {
                let _ = self.err_tx.send(err);
            }

            if let Some(request) = self.try_receive_request() {
                match self.handle(request) {
                    Ok(ControlFlow::Continue(())) => (),
                    Ok(ControlFlow::Break(())) => break,
                    Err(err) => {
                        // Propagate the error
                        let _ = self.err_tx.send(err);
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
