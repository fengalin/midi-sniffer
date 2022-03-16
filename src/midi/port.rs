use std::{collections::BTreeMap, fmt, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq)]
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

pub struct DirectionalPorts<T> {
    pub map: BTreeMap<Arc<str>, T>,
    pub cur: [Option<Arc<str>>; 2],
}

impl<T> Default for DirectionalPorts<T> {
    fn default() -> Self {
        Self {
            map: BTreeMap::new(),
            cur: [None, None],
        }
    }
}

impl<T> DirectionalPorts<T> {
    pub fn list(&self) -> impl Iterator<Item = &Arc<str>> {
        self.map.keys()
    }

    pub fn cur(&self, port_nb: PortNb) -> Option<&Arc<str>> {
        self.cur[port_nb.idx()].as_ref()
    }

    pub fn get(&self, port_name: &Arc<str>) -> Option<&T> {
        self.map.get(port_name)
    }

    fn update_from<M>(&mut self, client_name: &str, midi_io: &M) -> Result<(), midir::PortInfoError>
    where
        T: Clone,
        M: midir::MidiIO<Port = T>,
    {
        self.map.clear();

        let mut prev1 = self.cur[0].take();
        let mut prev2 = self.cur[1].take();
        for port in midi_io.ports() {
            let name = midi_io.port_name(&port)?;
            if !name.starts_with(client_name) {
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

    pub fn connected(&mut self, port_nb: PortNb, port_name: Arc<str>) {
        self.cur[port_nb.idx()] = Some(port_name);
    }

    pub fn disconnected(&mut self, port_nb: PortNb) {
        self.cur[port_nb.idx()] = None;
    }
}

pub struct Ports {
    pub ins: DirectionalPorts<midir::MidiInputPort>,
    pub client_name: Arc<str>,
}

impl Ports {
    pub fn new(client_name: &str) -> Self {
        Ports {
            ins: Default::default(),
            client_name: client_name.into(),
        }
    }

    pub fn update(&mut self, midi_in: &midir::MidiInput) -> Result<(), midir::PortInfoError> {
        let Ports { client_name, ins } = self;

        ins.update_from(client_name, midi_in)?;

        Ok(())
    }
}
