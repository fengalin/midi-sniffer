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

    fn update(&mut self, ctx: &egui::Context, frame: &epi::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("MIDI Sniffer");
            ui.add_space(10f32);

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    use super::port;
                    let resp = self.sniffer.ports_widget().show(ui);
                    match resp {
                        Some(port::Response::Connect(port_name)) => {
                            self.sniffer.connect(port_name);
                        }
                        Some(port::Response::Disconnect) => {
                            self.sniffer.disconnect();
                        }
                        None => (),
                    }

                    if ui.button("Refresh ports").clicked() {
                        self.sniffer.refresh_ports();
                    }
                });

                ui.separator();
                self.sniffer.msg_list_widget().show(ui);

                if let Some(err) = self.sniffer.pop_error() {
                    ui.separator();
                    ui.label(&format!("An error occured: {}", err));
                }
            });
        });

        frame.set_window_size(ctx.used_size());
    }

    fn on_exit(&mut self) {
        self.sniffer.shutdown();
    }
}
