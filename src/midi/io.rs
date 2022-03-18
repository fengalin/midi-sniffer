use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error connecting to {}", 0)]
    Connection(Arc<str>),
}

pub type MidiIn = MidiIO<midir::MidiInput, midir::MidiInputConnection<()>>;

pub enum MidiIO<IO: midir::MidiIO, C> {
    Connected(C),
    Disconnected(IO),
    None,
}

impl<IO: midir::MidiIO, C> Default for MidiIO<IO, C> {
    fn default() -> Self {
        Self::None
    }
}

impl<IO: midir::MidiIO, C> MidiIO<IO, C> {
    fn is_connected(&self) -> bool {
        matches!(self, Self::Connected(_))
    }
}

impl MidiIn {
    pub fn new(client_name: &str) -> Result<Self, midir::InitError> {
        Ok(Self::Disconnected(midir::MidiInput::new(client_name)?))
    }

    pub fn connect<C>(
        &mut self,
        port_name: Arc<str>,
        port: &midir::MidiInputPort,
        client_port_name: &str,
        mut callback: C,
    ) -> Result<(), Error>
    where
        C: FnMut(u64, &[u8]) + Send + 'static,
    {
        self.disconnect();
        match std::mem::take(self) {
            Self::Disconnected(midi_input) => {
                match midi_input.connect(
                    port,
                    client_port_name,
                    move |ts, buf, _port_name| callback(ts, buf),
                    (),
                ) {
                    Ok(conn) => {
                        *self = Self::Connected(conn);
                    }
                    Err(err) => {
                        *self = Self::Disconnected(err.into_inner());
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
                    *self = Self::Disconnected(io);
                }
                _ => unreachable!(),
            }
        }
    }
}
