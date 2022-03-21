use std::{error, fmt, sync::Arc};

#[derive(Debug)]
pub struct Origin {
    pub ts: u64,
    pub port_nb: super::PortNb,
    pub buffer: Arc<[u8]>,
}

impl Origin {
    pub fn new(ts: u64, port_nb: super::PortNb, buffer: &[u8]) -> Self {
        Self {
            ts,
            port_nb,
            buffer: buffer.into(),
        }
    }
}

#[derive(Debug)]
pub struct Msg {
    pub origin: Origin,
    pub msg: midi_msg::MidiMsg,
}

#[derive(Debug)]
pub struct Error {
    pub origin: Origin,
    pub err: midi_msg::ParseError,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} @ {} for {}",
            self.err, self.origin.ts, self.origin.port_nb
        )
    }
}

impl error::Error for Error {}

pub type Result = std::result::Result<Msg, self::Error>;
