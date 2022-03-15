use eframe::egui;
use once_cell::sync::Lazy;
use std::sync::Arc;

static DISCONNECTED: Lazy<Arc<str>> = Lazy::new(|| "Disconnected".into());

#[derive(Debug)]
pub struct DirectionalPorts {
    pub ports: Vec<Arc<str>>,
    cur: Arc<str>,
}

impl DirectionalPorts {
    fn update_from<T>(&mut self, ports: &crate::midi::DirectionalPorts<T>) {
        self.ports.clear();
        self.ports.extend(ports.list().cloned());

        self.cur = ports.cur().cloned().unwrap_or_else(|| DISCONNECTED.clone());
    }
}

impl Default for DirectionalPorts {
    fn default() -> Self {
        Self {
            ports: Vec::new(),
            cur: DISCONNECTED.clone(),
        }
    }
}

#[derive(Debug)]
pub enum Response {
    Connect(Arc<str>),
    Disconnect,
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
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Response> {
        let mut response = None;

        ui.horizontal(|ui| {
            ui.label("Port:");
            egui::ComboBox::from_label("")
                .selected_text(self.ins.cur.as_ref())
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(
                            &mut self.ins.cur,
                            DISCONNECTED.clone(),
                            DISCONNECTED.as_ref(),
                        )
                        .clicked()
                    {
                        response = Some(Response::Disconnect);
                    }

                    for port in self.ins.ports.iter() {
                        let mut layout_job = egui::text::LayoutJob::default();
                        layout_job.append(port.as_ref(), 0f32, egui::TextFormat::default());
                        /*
                        layout_job.text = port.to_string();
                        */
                        layout_job.justify = true;
                        if ui
                            //.selectable_value(&mut self.ins.cur, port.clone(), port.as_ref())
                            .selectable_value(&mut self.ins.cur, port.clone(), layout_job)
                            .clicked()
                        {
                            response = Some(Response::Connect(port.clone()));
                        }
                    }
                });
        });

        response
    }
}
