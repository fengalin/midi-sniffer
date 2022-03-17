use crossbeam_channel as channel;
use eframe::{egui, epi};
use std::{
    ops::ControlFlow,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::midi;

const MSG_POLLING_INTERVAL: Duration = Duration::from_millis(20);
const MSG_LIST_BATCH_SIZE: usize = 5;
const MAX_MSG_BATCHES_PER_UPDATE: usize = 30 / MSG_LIST_BATCH_SIZE;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("MIDI error: {}", 0)]
    Midi(#[from] super::port::Error),

    #[error("MIDI Message error: {}", 0)]
    Parse(#[from] crate::midi::msg::Error),
}

enum Request {
    Connect((midi::PortNb, Arc<str>)),
    Disconnect(midi::PortNb),
    HaveFrame(epi::Frame),
    RefreshPorts,
    Shutdown,
}

pub struct App {
    msg_list_widget: Arc<Mutex<super::MsgListWidget>>,
    to_tx: channel::Sender<Request>,
    from_rx: channel::Receiver<Error>,
    ports_widget: Arc<Mutex<super::PortsWidget>>,
    controller_thread: Option<std::thread::JoinHandle<()>>,
}

impl App {
    pub fn try_new(client_name: &str) -> Result<Self, Error> {
        let (from_tx, from_rx) = channel::unbounded();
        let (to_tx, to_rx) = channel::unbounded();

        let ports_widget = Arc::new(Mutex::new(super::PortsWidget::try_new(client_name)?));
        let msg_list_widget = Arc::new(Mutex::new(super::MsgListWidget::default()));

        let msg_list_widget_clone = msg_list_widget.clone();
        let ports_widget_clone = ports_widget.clone();
        let controller_thread = std::thread::spawn(move || {
            AppController::new(to_rx, from_tx, msg_list_widget_clone, ports_widget_clone).run()
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

impl epi::App for App {
    fn name(&self) -> &str {
        "MIDI Sniffer"
    }

    fn setup(
        &mut self,
        ctx: &egui::Context,
        frame: &epi::Frame,
        _storage: Option<&dyn epi::Storage>,
    ) {
        ctx.set_visuals(egui::Visuals::dark());
        self.have_frame(frame.clone());
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &epi::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("MIDI Sniffer");

            ui.add_space(10f32);

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    use super::port;
                    use crate::midi::PortNb;

                    let mut resp = self.ports_widget.lock().unwrap().show(PortNb::One, ui);

                    if ui.button("Refresh Ports").clicked() {
                        self.refresh_ports();
                    }

                    let resp2 = self.ports_widget.lock().unwrap().show(PortNb::Two, ui);
                    if resp2.is_some() {
                        resp = resp2;
                    }

                    match resp {
                        Some(port::Response::Connect((port_nb, port_name))) => {
                            self.connect(port_nb, port_name);
                        }
                        Some(port::Response::Disconnect(port_nb)) => {
                            self.disconnect(port_nb);
                        }
                        None => (),
                    }
                });

                ui.add_space(2f32);
                ui.separator();
                ui.add_space(2f32);

                self.msg_list_widget.lock().unwrap().show(ui);
            });

            if let Some(err) = self.pop_error() {
                ui.add_space(5f32);
                ui.group(|ui| {
                    ui.label(&format!("An error occured: {}", err));
                });
            }
        });
    }

    fn on_exit(&mut self) {
        self.shutdown();
    }
}

impl App {
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

    pub fn pop_error(&self) -> Option<Error> {
        match self.from_rx.try_recv() {
            Err(channel::TryRecvError::Empty) => None,
            Ok(err) => Some(err),
            Err(err) => panic!("{}", err),
        }
    }
}

struct AppController {
    msg_rx: channel::Receiver<midi::msg::Result>,
    msg_tx: channel::Sender<midi::msg::Result>,
    to_rx: channel::Receiver<Request>,
    from_tx: channel::Sender<Error>,
    msg_list_widget: Arc<Mutex<super::MsgListWidget>>,
    ports_widget: Arc<Mutex<super::PortsWidget>>,
    must_repaint: bool,
    frame: Option<epi::Frame>,
}

impl AppController {
    fn new(
        to_rx: channel::Receiver<Request>,
        from_tx: channel::Sender<Error>,
        msg_list_widget: Arc<Mutex<super::MsgListWidget>>,
        ports_widget: Arc<Mutex<super::PortsWidget>>,
    ) -> Self {
        let (msg_tx, msg_rx) = channel::unbounded();

        Self {
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

    fn connect(&mut self, port_nb: midi::PortNb, port_name: Arc<str>) -> Result<(), Error> {
        self.ports_widget
            .lock()
            .unwrap()
            .connect(port_nb, port_name, self.msg_tx.clone())?;

        Ok(())
    }

    fn try_receive_request(&mut self) -> Option<Request> {
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
        self.ports_widget
            .lock()
            .unwrap()
            .auto_connect(self.msg_tx.clone());

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
