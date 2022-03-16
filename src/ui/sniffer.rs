use crossbeam_channel as channel;
use eframe::epi;
use std::{
    ops::ControlFlow,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use crate::midi;

const MSG_POLLING_INTERVAL: Duration = Duration::from_millis(20);
const MSG_LIST_BATCH_SIZE: usize = 5;
const MAX_MSG_BATCHES_PER_UPDATE: usize = 30 / MSG_LIST_BATCH_SIZE;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("MIDI Sniffer error: {}", 0)]
    Midi(#[from] crate::SnifferError),

    #[error("MIDI Message error: {}", 0)]
    Parse(#[from] crate::MidiMsgError),
}

enum Request {
    Connect((midi::PortNb, Arc<str>)),
    Disconnect(midi::PortNb),
    HaveFrame(epi::Frame),
    RefreshPorts,
    Shutdown,
}

pub type MidiMsgParseResult = Result<crate::MidiMsg, crate::MidiMsgError>;

pub struct Sniffer {
    msg_list_widget: Arc<Mutex<super::MsgListWidget>>,
    to_tx: channel::Sender<Request>,
    from_rx: channel::Receiver<Error>,
    ports_widget: Arc<Mutex<super::PortsWidget>>,
    controller_thread: Option<std::thread::JoinHandle<()>>,
}

impl Sniffer {
    pub fn try_new(client_name: &str) -> Result<Self, Error> {
        let (from_tx, from_rx) = channel::unbounded();
        let (to_tx, to_rx) = channel::unbounded();

        let sniffer = crate::Sniffer::try_new(client_name)?;
        let msg_list_widget = Arc::new(Mutex::new(super::MsgListWidget::default()));
        let ports_widget = Arc::new(Mutex::new(super::PortsWidget::new(&sniffer.ports)));

        let msg_list_widget_clone = msg_list_widget.clone();
        let ports_widget_clone = ports_widget.clone();
        let controller_thread = std::thread::spawn(move || {
            SnifferController::new(
                sniffer,
                to_rx,
                from_tx,
                msg_list_widget_clone,
                ports_widget_clone,
            )
            .run()
        });

        Ok(Self {
            msg_list_widget,
            to_tx,
            from_rx,
            ports_widget,
            controller_thread: Some(controller_thread),
        })
    }
}

impl Sniffer {
    pub fn shutdown(&mut self) {
        if let Some(controller_thread) = self.controller_thread.take() {
            if let Err(err) = self.to_tx.send(Request::Shutdown) {
                log::error!("Sniffer couldn't request shutdown: {}", err);
            } else {
                let _ = controller_thread.join();
            }
        }
    }

    pub fn refresh_ports(&self) {
        self.to_tx.send(Request::RefreshPorts).unwrap();
    }

    pub fn connect(&self, port_nb: midi::PortNb, port_name: Arc<str>) {
        self.to_tx
            .send(Request::Connect((port_nb, port_name)))
            .unwrap();
    }

    pub fn disconnect(&self, port_nb: midi::PortNb) {
        self.to_tx.send(Request::Disconnect(port_nb)).unwrap();
    }

    pub fn have_frame(&self, frame: epi::Frame) {
        self.to_tx.send(Request::HaveFrame(frame)).unwrap();
    }
}

impl Sniffer {
    pub fn msg_list_widget(&self) -> MutexGuard<'_, super::MsgListWidget> {
        self.msg_list_widget.lock().unwrap()
    }

    pub fn ports_widget(&self) -> MutexGuard<'_, super::PortsWidget> {
        self.ports_widget.lock().unwrap()
    }

    pub fn pop_error(&self) -> Option<Error> {
        match self.from_rx.try_recv() {
            Err(channel::TryRecvError::Empty) => None,
            Ok(err) => Some(err),
            Err(err) => panic!("{}", err),
        }
    }
}

struct SnifferController {
    sniffer: crate::Sniffer,
    is_connected: bool,
    msg_rx: channel::Receiver<MidiMsgParseResult>,
    msg_tx: channel::Sender<MidiMsgParseResult>,
    to_rx: channel::Receiver<Request>,
    from_tx: channel::Sender<Error>,
    msg_list_widget: Arc<Mutex<super::MsgListWidget>>,
    ports_widget: Arc<Mutex<super::PortsWidget>>,
    must_repaint: bool,
    frame: Option<epi::Frame>,
}

impl SnifferController {
    fn new(
        sniffer: crate::Sniffer,
        to_rx: channel::Receiver<Request>,
        from_tx: channel::Sender<Error>,
        msg_list_widget: Arc<Mutex<super::MsgListWidget>>,
        ports_widget: Arc<Mutex<super::PortsWidget>>,
    ) -> Self {
        let (msg_tx, msg_rx) = channel::unbounded();

        Self {
            sniffer,
            is_connected: false,
            msg_rx,
            msg_tx,
            to_rx,
            from_tx,
            msg_list_widget,
            ports_widget,
            must_repaint: false,
            frame: None,
        }
    }

    fn handle_request(&mut self, request: Request) -> Result<ControlFlow<(), ()>, Error> {
        use self::Request::*;
        match request {
            Connect((port_nb, port_name)) => {
                self.connect(port_nb, port_name)?;
                self.refresh_ports();
            }
            Disconnect(port_nb) => {
                self.sniffer.disconnect(port_nb)?;
                self.is_connected = false;
                self.refresh_ports();
            }
            RefreshPorts => {
                self.sniffer.refresh_ports()?;
                self.refresh_ports();
            }
            Shutdown => return Ok(ControlFlow::Break(())),
            HaveFrame(egui_frame) => {
                self.frame = Some(egui_frame);
            }
        }

        Ok(ControlFlow::Continue(()))
    }

    fn connect(&mut self, port_nb: midi::PortNb, port_name: Arc<str>) -> Result<(), Error> {
        let msg_tx = self.msg_tx.clone();
        self.sniffer.connect(
            port_nb,
            port_name,
            move |ts, buf| match midi_msg::MidiMsg::from_midi(buf) {
                Ok((msg, _len)) => {
                    msg_tx
                        .send(Ok(crate::MidiMsg { ts, port_nb, msg }))
                        .unwrap();
                }
                Err(err) => {
                    log::error!("Failed to parse Midi buffer: {}", err);
                    msg_tx
                        .send(Err(crate::MidiMsgError { ts, port_nb, err }))
                        .unwrap();
                }
            },
        )?;

        self.is_connected = true;
        Ok(())
    }

    fn auto_connect(&mut self) {
        // Auto-connect to first available port
        // FIXME save the last connected port and try to connect it again on startup

        let ports_widget = self.ports_widget.clone();
        let mut ports_widget = ports_widget.lock().unwrap();
        for port_name in ports_widget.ins.ports.iter() {
            if self.connect(midi::PortNb::One, port_name.clone()).is_ok() {
                ports_widget.update_from(&self.sniffer.ports);
                break;
            }
        }
    }

    fn refresh_ports(&self) {
        self.ports_widget
            .lock()
            .unwrap()
            .update_from(&self.sniffer.ports);
    }

    fn try_receive_request(&mut self) -> Option<Request> {
        if !self.is_connected {
            match self.to_rx.recv() {
                Ok(request) => return Some(request),
                Err(err) => panic!("{}", err),
            }
        }

        let request = self
            .to_rx
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
        self.auto_connect();

        loop {
            if let Some(request) = self.try_receive_request() {
                match self.handle_request(request) {
                    Ok(ControlFlow::Continue(())) => (),
                    Ok(ControlFlow::Break(())) => break,
                    Err(err) => {
                        // Propagate the error
                        let _ = self.from_tx.send(err);
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

        log::info!("Shuting down Sniffer Controller loop");
    }
}
