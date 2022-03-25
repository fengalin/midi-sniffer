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
    pub ports_widget: Arc<Mutex<super::PortsWidget>>,
}

impl Spawner {
    pub fn spawn(self) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            Controller::new(
                self.req_rx,
                self.err_tx,
                self.msg_list_widget,
                self.ports_widget,
            )
            .run()
        })
    }
}

struct Controller {
    msg_rx: channel::Receiver<midi::msg::Result>,
    msg_tx: channel::Sender<midi::msg::Result>,
    req_rx: channel::Receiver<app::Request>,
    err_tx: channel::Sender<app::Error>,
    msg_list_widget: Arc<Mutex<super::MsgListWidget>>,
    ports_widget: Arc<Mutex<super::PortsWidget>>,
    must_repaint: bool,
    frame: Option<epi::Frame>,
}

impl Controller {
    fn new(
        req_rx: channel::Receiver<app::Request>,
        err_tx: channel::Sender<app::Error>,
        msg_list_widget: Arc<Mutex<super::MsgListWidget>>,
        ports_widget: Arc<Mutex<super::PortsWidget>>,
    ) -> Self {
        let (msg_tx, msg_rx) = channel::unbounded();

        Self {
            msg_rx,
            msg_tx,
            req_rx,
            err_tx,
            msg_list_widget,
            ports_widget,
            must_repaint: false,
            frame: None,
        }
    }

    fn handle_request(&mut self, request: app::Request) -> Result<ControlFlow<(), ()>, app::Error> {
        use app::Request::*;
        match request {
            Connect((port_nb, port_name)) => {
                self.connect(port_nb, port_name)?;
            }
            Disconnect(port_nb) => {
                self.ports_widget.lock().unwrap().disconnect(port_nb)?;
            }
            RefreshPorts => {
                self.ports_widget.lock().unwrap().refresh_ports()?;
            }
            Shutdown => return Ok(ControlFlow::Break(())),
            HaveFrame(egui_frame) => {
                self.frame = Some(egui_frame);
            }
        }

        Ok(ControlFlow::Continue(()))
    }

    fn connect(&mut self, port_nb: midi::PortNb, port_name: Arc<str>) -> Result<(), app::Error> {
        self.ports_widget
            .lock()
            .unwrap()
            .connect(port_nb, port_name, self.msg_tx.clone())?;

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
            if let Some(request) = self.try_receive_request() {
                match self.handle_request(request) {
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
