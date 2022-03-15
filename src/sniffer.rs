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

#[derive(Debug)]
pub struct MidiMsg {
    pub ts: u64,
    pub msg: midi_msg::MidiMsg,
}

pub struct Sniffer {
    midi_in: crate::MidiIn,
    pub ports: midi::Ports,
}

impl Sniffer {
    pub fn try_new(client_name: &str) -> Result<Self, SnifferError> {
        let midi_in = crate::MidiIn::new(client_name)?;

        let mut ports = midi::Ports::new(client_name);
        ports.update(midi_in.io().unwrap())?;

        Ok(Self { midi_in, ports })
    }

    pub fn refresh_ports(&mut self) -> Result<(), SnifferError> {
        self.ports.update(&midir::MidiInput::new(&format!(
            "{} referesh ports",
            self.ports.client_name.as_ref(),
        ))?)?;

        Ok(())
    }

    pub fn connect<C>(&mut self, port_name: Arc<str>, callback: C) -> Result<(), SnifferError>
    where
        C: Fn(u64, &[u8]) + Send + 'static,
    {
        let port = self
            .ports
            .ins
            .get(&port_name)
            .ok_or_else(|| SnifferError::PortNotFound(port_name.clone()))?;

        self.midi_in
            .connect(port, &format!("{} Input", self.ports.client_name), callback)
            .map_err(|_| {
                self.ports.ins.disconnected();
                SnifferError::PortConnection
            })?;
        self.ports.ins.connected(port_name);

        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<(), SnifferError> {
        self.midi_in.disconnect();
        self.ports.ins.disconnected();

        Ok(())
    }
}
