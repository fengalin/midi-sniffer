#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error connecting to {}", 0)]
    Connection(String),
}

pub type MidiIn = MidiIO<midir::MidiInput, midir::MidiInputConnection<()>, ()>;

pub enum MidiIO<IO: midir::MidiIO, C, D> {
    Connected(C),
    Disconnected((IO, D)),
    None,
}

impl<IO: midir::MidiIO, C, D> Default for MidiIO<IO, C, D> {
    fn default() -> Self {
        Self::None
    }
}

impl<IO: midir::MidiIO, C, D> MidiIO<IO, C, D> {
    pub fn io(&self) -> Option<&IO> {
        match self {
            Self::Disconnected((io, _d)) => Some(io),
            _ => None,
        }
    }

    pub fn conn(&mut self) -> Option<&mut C> {
        match self {
            Self::Connected(conn) => Some(conn),
            _ => None,
        }
    }

    fn is_connected(&self) -> bool {
        matches!(self, Self::Connected(_))
    }
}

impl MidiIn {
    pub fn new(client_name: &str) -> Result<Self, midir::InitError> {
        Ok(Self::Disconnected((
            midir::MidiInput::new(client_name)?,
            (),
        )))
    }

    pub fn connect<C>(
        &mut self,
        port: &midir::MidiInputPort,
        client_port_name: &str,
        mut callback: C,
    ) -> Result<(), Error>
    where
        C: FnMut(u64, &[u8]) + Send + 'static,
    {
        self.disconnect();
        match std::mem::take(self) {
            Self::Disconnected((midi_input, _)) => {
                let port_name = midi_input.port_name(port).unwrap();
                match midi_input.connect(
                    port,
                    client_port_name,
                    move |ts, buf, _| callback(ts, buf),
                    (),
                ) {
                    Ok(conn) => {
                        log::info!("Connected Input to {}", port_name);
                        *self = Self::Connected(conn);
                    }
                    Err(err) => {
                        *self = Self::Disconnected((err.into_inner(), ()));
                        let err = Error::Connection(port_name);
                        log::error!("{}", err);
                        return Err(err);
                    }
                };
            }
            _ => unreachable!(),
        }

        Ok(())
    }

    pub fn disconnect(&mut self) {
        if self.is_connected() {
            match std::mem::take(self) {
                Self::Connected(conn) => {
                    let (io, _) = conn.close();
                    *self = Self::Disconnected((io, ()));
                    log::debug!("Disconnected Input");
                }
                _ => unreachable!(),
            }
        }
    }
}
