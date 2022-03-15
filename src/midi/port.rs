use std::collections::BTreeMap;
use std::sync::Arc;

pub struct DirectionalPorts<T> {
    pub map: BTreeMap<Arc<str>, T>,
    pub cur: Option<Arc<str>>,
}

impl<T> Default for DirectionalPorts<T> {
    fn default() -> Self {
        Self {
            map: BTreeMap::new(),
            cur: None,
        }
    }
}

impl<T> DirectionalPorts<T> {
    pub fn list(&self) -> impl Iterator<Item = &Arc<str>> {
        self.map.keys()
    }

    pub fn cur(&self) -> Option<&Arc<str>> {
        self.cur.as_ref()
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

        let mut prev = self.cur.take();
        for port in midi_io.ports() {
            let name = midi_io.port_name(&port)?;
            if !name.starts_with(client_name) {
                #[cfg(feature = "jack")]
                let name = name.strip_prefix("Midi-Bridge:").unwrap_or(&name);

                if let Some(ref cur) = prev {
                    if cur.as_ref() == name {
                        self.cur = prev.take();
                    }
                }

                self.map.insert(name.into(), port);
            }
        }

        Ok(())
    }

    pub fn connected(&mut self, port_name: Arc<str>) {
        self.cur = Some(port_name);
    }

    pub fn disconnected(&mut self) {
        self.cur = None;
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
