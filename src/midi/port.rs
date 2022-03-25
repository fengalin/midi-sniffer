use std::{collections::BTreeMap, fmt, sync::Arc};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Midi initialization failed")]
    Init(#[from] midir::InitError),

    #[error("Failed to parse Midi message")]
    ParseError(#[from] midi_msg::ParseError),

    #[error("Midi port connection failed")]
    PortConnection,

    #[error("Couldn't retrieve a port name")]
    PortInfoError(#[from] midir::PortInfoError),

    #[error("Invalid Midi port name {}", .0)]
    PortNotFound(Arc<str>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize))]
pub enum PortNb {
    One,
    Two,
}

impl fmt::Display for PortNb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PortNb {
    pub fn idx(self) -> usize {
        match self {
            PortNb::One => 0,
            PortNb::Two => 1,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            PortNb::One => "Port 1",
            PortNb::Two => "Port 2",
        }
    }

    pub fn as_char(&self) -> char {
        match self {
            PortNb::One => '1',
            PortNb::Two => '2',
        }
    }
}

pub struct Ports {
    pub map: BTreeMap<Arc<str>, midir::MidiInputPort>,
    pub cur: [Option<Arc<str>>; 2],
    midi_in: [crate::MidiIn; 2],
    pub client_name: Arc<str>,
}

impl Ports {
    pub fn try_new(client_name: Arc<str>) -> Result<Self, Error> {
        let midi_in1 = crate::MidiIn::new(&client_name)?;
        let midi_in2 = crate::MidiIn::new(&client_name)?;

        Ok(Self {
            map: BTreeMap::new(),
            cur: [None, None],
            midi_in: [midi_in1, midi_in2],
            client_name,
        })
    }

    pub fn list(&self) -> impl Iterator<Item = &Arc<str>> {
        self.map.keys()
    }

    pub fn cur(&self, port_nb: PortNb) -> Option<&Arc<str>> {
        self.cur[port_nb.idx()].as_ref()
    }

    fn midi_in_mut(&mut self, port_nb: super::PortNb) -> &mut crate::MidiIn {
        &mut self.midi_in[port_nb.idx()]
    }

    pub fn refresh(&mut self) -> Result<(), Error> {
        let temp_midi_in =
            midir::MidiInput::new(&format!("{} referesh ports", self.client_name.as_ref()))?;

        self.map.clear();

        let mut prev1 = self.cur[0].take();
        let mut prev2 = self.cur[1].take();
        for port in temp_midi_in.ports() {
            let name = temp_midi_in.port_name(&port)?;
            if !name.starts_with(self.client_name.as_ref()) {
                #[cfg(feature = "jack")]
                let name = name.strip_prefix("Midi-Bridge:").unwrap_or(&name);

                if let Some(ref prev1_ref) = prev1 {
                    if prev1_ref.as_ref() == name {
                        self.cur[0] = prev1.take();
                    }
                }

                if let Some(ref prev2_ref) = prev2 {
                    if prev2_ref.as_ref() == name {
                        self.cur[1] = prev2.take();
                    }
                }

                self.map.insert(name.into(), port);
            }
        }

        Ok(())
    }

    pub fn connect<C>(
        &mut self,
        port_nb: super::PortNb,
        port_name: Arc<str>,
        callback: C,
    ) -> Result<(), Error>
    where
        C: FnMut(u64, &[u8]) + Send + 'static,
    {
        let port = self
            .map
            .get(&port_name)
            .ok_or_else(|| Error::PortNotFound(port_name.clone()))?
            .clone();

        let app_port_name = format!("{} {}", self.client_name, port_nb);
        self.midi_in_mut(port_nb)
            .connect(port_name.clone(), &port, &app_port_name, callback)
            .map_err(|_| {
                self.cur[port_nb.idx()] = None;
                Error::PortConnection
            })?;

        log::info!("Connected Input {} to {}", port_nb, port_name);
        self.cur[port_nb.idx()] = Some(port_name);
        self.refresh()?;

        Ok(())
    }

    pub fn disconnect(&mut self, port_nb: super::PortNb) -> Result<(), Error> {
        self.midi_in_mut(port_nb).disconnect();

        if let Some(port_name) = self.cur[port_nb.idx()].take() {
            log::info!("Disconnected Input {} from {}", port_nb, port_name);
        }
        self.refresh()?;

        Ok(())
    }
}
