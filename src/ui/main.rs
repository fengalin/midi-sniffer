use eframe::{egui, epi};

pub struct Main {
    sniffer: super::Sniffer,
}

impl Main {
    pub fn try_new(client_name: &str) -> Result<Self, super::sniffer::Error> {
        Ok(Self {
            sniffer: super::Sniffer::try_new(client_name)?,
        })
    }
}

impl epi::App for Main {
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
        self.sniffer.have_frame(frame.clone());
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &epi::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("MIDI Sniffer");

            ui.add_space(10f32);

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    use super::port;
                    use crate::midi::PortNb;

                    let mut resp = self.sniffer.ports_widget().show(PortNb::One, ui);

                    if ui.button("Refresh Ports").clicked() {
                        self.sniffer.refresh_ports();
                    }

                    let resp2 = self.sniffer.ports_widget().show(PortNb::Two, ui);
                    if resp2.is_some() {
                        resp = resp2;
                    }

                    match resp {
                        Some(port::Response::Connect((port_nb, port_name))) => {
                            self.sniffer.connect(port_nb, port_name);
                        }
                        Some(port::Response::Disconnect(port_nb)) => {
                            self.sniffer.disconnect(port_nb);
                        }
                        None => (),
                    }
                });

                ui.add_space(2f32);
                ui.separator();
                ui.add_space(2f32);

                self.sniffer.msg_list_widget().show(ui);
            });

            if let Some(err) = self.sniffer.pop_error() {
                ui.add_space(5f32);
                ui.group(|ui| {
                    ui.label(&format!("An error occured: {}", err));
                });
            }
        });
    }

    fn on_exit(&mut self) {
        self.sniffer.shutdown();
    }
}
