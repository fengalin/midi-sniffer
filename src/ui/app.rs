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
    #[error("{}", .0)]
    Midi(#[from] super::port::Error),

    #[error("Failed to parse MIDI Message")]
    Parse(#[from] crate::midi::msg::Error),

    #[error("{}", .0)]
    MsgList(#[from] super::msg_list::Error),
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
    req_tx: channel::Sender<Request>,
    err_rx: channel::Receiver<Error>,
    ports_widget: Arc<Mutex<super::PortsWidget>>,
    last_err: Option<Error>,
    controller_thread: Option<std::thread::JoinHandle<()>>,
}

impl App {
    pub fn try_new(client_name: &str) -> Result<Self, Error> {
        let (err_tx, err_rx) = channel::unbounded();
        let (req_tx, req_rx) = channel::unbounded();

        let ports_widget = Arc::new(Mutex::new(super::PortsWidget::try_new(client_name)?));
        let msg_list_widget = Arc::new(Mutex::new(super::MsgListWidget::new(err_tx.clone())));

        let msg_list_widget_clone = msg_list_widget.clone();
        let ports_widget_clone = ports_widget.clone();
        let controller_thread = std::thread::spawn(move || {
            Controller::new(req_rx, err_tx, msg_list_widget_clone, ports_widget_clone).run()
        });

        Ok(Self {
            msg_list_widget,
            req_tx,
            err_rx,
            ports_widget,
            last_err: None,
            controller_thread: Some(controller_thread),
        })
    }
}

impl epi::App for App {
    fn name(&self) -> &str {
        "midi-sniffer"
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &epi::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("MIDI Sniffer");

            ui.add_space(10f32);

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    use crate::midi::PortNb;

                    let resp1 = self.ports_widget.lock().unwrap().show(PortNb::One, ui);
                    let resp2 = self.ports_widget.lock().unwrap().show(PortNb::Two, ui);

                    Dispatcher::<super::PortsWidget>::dispatch(self, resp1.or(resp2));
                });

                ui.add_space(2f32);
                ui.separator();
                ui.add_space(2f32);

                self.msg_list_widget.lock().unwrap().show(ui);
            });

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
    }

    fn setup(
        &mut self,
        ctx: &egui::Context,
        frame: &epi::Frame,
        storage: Option<&dyn epi::Storage>,
    ) {
        ctx.set_visuals(egui::Visuals::dark());
        self.have_frame(frame.clone());

        let resps = self.ports_widget.lock().unwrap().setup(storage);
        for resp in resps {
            Dispatcher::<super::PortsWidget>::dispatch(self, Some(resp));
        }

        self.msg_list_widget.lock().unwrap().setup(storage);
    }

    fn save(&mut self, storage: &mut dyn epi::Storage) {
        self.ports_widget.lock().unwrap().save(storage);
        self.msg_list_widget.lock().unwrap().save(storage);
        self.clear_last_err();
    }

    fn on_exit(&mut self) {
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

    pub fn have_frame(&self, frame: epi::Frame) {
        self.req_tx.send(Request::HaveFrame(frame)).unwrap();
    }

    fn clear_last_err(&mut self) {
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

struct Dispatcher<T>(std::marker::PhantomData<*const T>);

impl Dispatcher<super::PortsWidget> {
    fn dispatch(app: &mut App, resp: Option<super::port::Response>) {
        if let Some(resp) = resp {
            use super::port::Response::*;

            app.clear_last_err();
            app.req_tx.send(Request::RefreshPorts).unwrap();

            match resp {
                Connect((port_nb, port_name)) => {
                    app.req_tx
                        .send(Request::Connect((port_nb, port_name)))
                        .unwrap();
                }
                Disconnect(port_nb) => {
                    app.req_tx.send(Request::Disconnect(port_nb)).unwrap();
                }
                CheckingList => (), // only refresh ports & clear last_err
            }
        }
    }
}

struct Controller {
    msg_rx: channel::Receiver<midi::msg::Result>,
    msg_tx: channel::Sender<midi::msg::Result>,
    req_rx: channel::Receiver<Request>,
    err_tx: channel::Sender<Error>,
    msg_list_widget: Arc<Mutex<super::MsgListWidget>>,
    ports_widget: Arc<Mutex<super::PortsWidget>>,
    must_repaint: bool,
    frame: Option<epi::Frame>,
}

impl Controller {
    fn new(
        req_rx: channel::Receiver<Request>,
        err_tx: channel::Sender<Error>,
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
