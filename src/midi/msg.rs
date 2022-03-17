use std::{error, fmt};

#[derive(Debug)]
pub struct Msg {
    pub ts: u64,
    pub port_nb: super::PortNb,
    pub msg: midi_msg::MidiMsg,
}

#[derive(Debug)]
pub struct Error {
    pub ts: u64,
    pub port_nb: super::PortNb,
    pub err: midi_msg::ParseError,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} @ {} for {}", self.err, self.ts, self.port_nb)
    }
}

impl error::Error for Error {}

pub type Result = std::result::Result<Msg, self::Error>;
