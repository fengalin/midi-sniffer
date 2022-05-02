use eframe::{self, egui};
use once_cell::sync::Lazy;
use std::sync::Arc;

use crate::midi;

static DISCONNECTED: Lazy<Arc<str>> = Lazy::new(|| "Disconnected".into());
const STORAGE_PORT_1: &str = "port_1";
const STORAGE_PORT_2: &str = "port_2";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to parse Midi message")]
    ParseError(#[from] midi_msg::ParseError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct UniquePort {
    nb: midi::PortNb,
    name: Arc<str>,
}

impl UniquePort {
    fn disconnected(port_nb: midi::PortNb) -> Self {
        UniquePort {
            nb: port_nb,
            name: DISCONNECTED.clone(),
        }
    }
}

#[derive(Debug)]
pub struct DirectionalPortView<'a> {
    pub list: &'a Vec<Arc<str>>,
    port_nb: midi::PortNb,
    cur: Arc<str>,
}

impl<'a> DirectionalPortView<'a> {
    fn unique_ports_iter(&self) -> impl Iterator<Item = UniquePort> + '_ {
        self.list.iter().cloned().map(|name| UniquePort {
            nb: self.port_nb,
            name,
        })
    }

    fn cur(&self) -> UniquePort {
        UniquePort {
            nb: self.port_nb,
            name: self.cur.clone(),
        }
    }
}

#[derive(Debug)]
pub struct DirectionalPorts {
    pub list: Vec<Arc<str>>,
    cur: [Arc<str>; 2],
}

impl DirectionalPorts {
    fn view(&self, port_nb: midi::PortNb) -> DirectionalPortView {
        DirectionalPortView {
            list: &self.list,
            port_nb,
            cur: self.cur[port_nb.idx()].clone(),
        }
    }

    fn update_from(&mut self, ports: &midi::Ports) {
        self.list.clear();
        self.list.extend(ports.list().cloned());

        self.update_cur(midi::PortNb::One, ports);
        self.update_cur(midi::PortNb::Two, ports);
    }

    fn update_cur(&mut self, port_nb: midi::PortNb, ports: &midi::Ports) {
        self.cur[port_nb.idx()] = ports
            .cur(port_nb)
            .cloned()
            .unwrap_or_else(|| DISCONNECTED.clone());
    }
}

impl Default for DirectionalPorts {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            cur: [DISCONNECTED.clone(), DISCONNECTED.clone()],
        }
    }
}

#[derive(Debug)]
pub enum Response {
    Connect((midi::PortNb, Arc<str>)),
    Disconnect(midi::PortNb),
    CheckingList,
}

#[derive(Default)]
pub struct PortsPanel {
    pub ports: DirectionalPorts,
}

impl PortsPanel {
    pub fn setup(storage: Option<&dyn eframe::Storage>) -> impl Iterator<Item = Response> {
        use Response::*;

        let mut resp = Vec::new();
        if let Some(storage) = storage {
            if let Some(port) = storage.get_string(STORAGE_PORT_1) {
                if port != DISCONNECTED.as_ref() {
                    resp.push(Connect((midi::PortNb::One, port.into())));
                }
            }
            if let Some(port) = storage.get_string(STORAGE_PORT_2) {
                if port != DISCONNECTED.as_ref() {
                    resp.push(Connect((midi::PortNb::Two, port.into())));
                }
            }
        }

        resp.into_iter()
    }

    #[must_use]
    pub fn show(&mut self, port_nb: midi::PortNb, ui: &mut egui::Ui) -> Option<Response> {
        use Response::*;

        let view = self.ports.view(port_nb);
        let mut selected = view.cur();

        let resp = egui::ComboBox::from_label(port_nb.as_str())
            .selected_text(view.cur.as_ref())
            .show_ui(ui, |ui| {
                let mut resp = None;

                if ui
                    .selectable_value(
                        &mut selected,
                        UniquePort::disconnected(port_nb),
                        DISCONNECTED.as_ref(),
                    )
                    .clicked()
                {
                    resp = Some(Disconnect(port_nb));
                }

                for port in view.unique_ports_iter() {
                    if ui
                        .selectable_value(&mut selected, port.clone(), port.name.as_ref())
                        .clicked()
                    {
                        resp = Some(Connect((port_nb, port.name)));
                    }
                }

                resp
            })
            .inner;

        if let Some(None) = resp {
            Some(CheckingList)
        } else {
            resp.flatten()
        }
    }

    pub fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(
            STORAGE_PORT_1,
            self.ports.cur[midi::PortNb::One.idx()].to_string(),
        );
        storage.set_string(
            STORAGE_PORT_2,
            self.ports.cur[midi::PortNb::Two.idx()].to_string(),
        );
    }
}

impl PortsPanel {
    pub fn update(&mut self, midi_ports: &midi::Ports) {
        self.ports.update_from(midi_ports);
    }
}
