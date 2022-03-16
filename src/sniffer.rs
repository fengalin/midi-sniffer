use std::sync::Arc;

use crate::midi;

#[derive(Debug, thiserror::Error)]
pub enum SnifferError {
    #[error("Sniffer initialization failed")]
    Init(#[from] midir::InitError),

    #[error("Sniffer failed to parse Midi message")]
    ParseError(#[from] midi_msg::ParseError),

    #[error("Sniffer input port creation failed")]
    PortCreation,

    #[error("Sniffer port connection failed")]
    PortConnection,

    #[error("Sniffer couldn't retrieve a port name")]
    PortInfoError(#[from] midir::PortInfoError),

    #[error("Sniffer invalid port name {}", 0)]
    PortNotFound(Arc<str>),
}

pub struct Sniffer {
    midi_in: [crate::MidiIn; 2],
    pub ports: midi::Ports,
}

impl Sniffer {
    pub fn try_new(client_name: &str) -> Result<Self, SnifferError> {
        let midi_in1 = crate::MidiIn::new(client_name)?;

        let mut ports = midi::Ports::new(client_name);
        ports.update(midi_in1.io().unwrap())?;

        let midi_in2 = crate::MidiIn::new(client_name)?;

        Ok(Self {
            midi_in: [midi_in1, midi_in2],
            ports,
        })
    }

    pub fn midi_in_mut(&mut self, port_nb: midi::PortNb) -> &mut crate::MidiIn {
        &mut self.midi_in[port_nb.idx()]
    }

    pub fn refresh_ports(&mut self) -> Result<(), SnifferError> {
        self.ports.update(&midir::MidiInput::new(&format!(
            "{} referesh ports",
            self.ports.client_name.as_ref(),
        ))?)?;

        Ok(())
    }

    pub fn connect<C>(
        &mut self,
        port_nb: midi::PortNb,
        port_name: Arc<str>,
        callback: C,
    ) -> Result<(), SnifferError>
    where
        C: Fn(u64, &[u8]) + Send + 'static,
    {
        let port = self
            .ports
            .ins
            .get(&port_name)
            .ok_or_else(|| SnifferError::PortNotFound(port_name.clone()))?
            .clone();

        let app_port_name = format!("{} {}", self.ports.client_name, port_nb);
        self.midi_in_mut(port_nb)
            .connect(port_name.clone(), &port, &app_port_name, callback)
            .map_err(|_| {
                self.ports.ins.disconnected(port_nb);
                SnifferError::PortConnection
            })?;
        self.ports.ins.connected(port_nb, port_name);

        Ok(())
    }

    pub fn disconnect(&mut self, port_nb: midi::PortNb) -> Result<(), SnifferError> {
        self.midi_in_mut(port_nb).disconnect();
        self.ports.ins.disconnected(port_nb);

        Ok(())
    }
}
