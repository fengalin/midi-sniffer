use eframe::egui;
use once_cell::sync::Lazy;
use std::sync::Arc;

use crate::midi;

static DISCONNECTED: Lazy<Arc<str>> = Lazy::new(|| "Disconnected".into());

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
    pub ports: &'a Vec<Arc<str>>,
    port_nb: midi::PortNb,
    cur: Arc<str>,
}

impl<'a> DirectionalPortView<'a> {
    fn unique_ports_iter(&self) -> impl Iterator<Item = UniquePort> + '_ {
        self.ports.iter().cloned().map(|name| UniquePort {
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
    pub ports: Vec<Arc<str>>,
    cur: [Arc<str>; 2],
}

impl DirectionalPorts {
    fn view(&self, port_nb: midi::PortNb) -> DirectionalPortView {
        DirectionalPortView {
            ports: &self.ports,
            port_nb,
            cur: self.cur[port_nb.idx()].clone(),
        }
    }

    fn update_from<T>(&mut self, ports: &crate::midi::DirectionalPorts<T>) {
        self.ports.clear();
        self.ports.extend(ports.list().cloned());

        self.update_cur(midi::PortNb::One, ports);
        self.update_cur(midi::PortNb::Two, ports);
    }

    fn update_cur<T>(&mut self, port_nb: midi::PortNb, ports: &crate::midi::DirectionalPorts<T>) {
        self.cur[port_nb.idx()] = ports
            .cur(port_nb)
            .cloned()
            .unwrap_or_else(|| DISCONNECTED.clone());
    }
}

impl Default for DirectionalPorts {
    fn default() -> Self {
        Self {
            ports: Vec::new(),
            cur: [DISCONNECTED.clone(), DISCONNECTED.clone()],
        }
    }
}

#[derive(Debug)]
pub enum Response {
    Connect((midi::PortNb, Arc<str>)),
    Disconnect(midi::PortNb),
}

#[derive(Debug)]
pub struct PortsWidget {
    pub ins: DirectionalPorts,
}

impl PortsWidget {
    pub fn new(midi_ports: &crate::midi::Ports) -> Self {
        let mut ports_widget = PortsWidget {
            ins: DirectionalPorts::default(),
        };

        ports_widget.update_from(midi_ports);
        ports_widget
    }

    pub fn update_from(&mut self, midi_ports: &crate::midi::Ports) {
        self.ins.update_from(&midi_ports.ins);
    }

    #[must_use]
    pub fn show(&mut self, port_nb: midi::PortNb, ui: &mut egui::Ui) -> Option<Response> {
        let mut response = None;

        let view = self.ins.view(port_nb);
        let mut selected = view.cur();

        egui::ComboBox::from_label(port_nb.as_str())
            .selected_text(view.cur.as_ref())
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(
                        &mut selected,
                        UniquePort::disconnected(port_nb),
                        DISCONNECTED.as_ref(),
                    )
                    .clicked()
                {
                    response = Some(Response::Disconnect(port_nb));
                }

                for port in view.unique_ports_iter() {
                    if ui
                        .selectable_value(&mut selected, port.clone(), port.name.as_ref())
                        .clicked()
                    {
                        response = Some(Response::Connect((port_nb, port.name)));
                    }
                }
            });

        response
    }
}
