use super::{app, App};

pub struct Dispatcher<T>(std::marker::PhantomData<*const T>);

impl Dispatcher<super::PortsPanel> {
    pub fn handle(app: &mut App, resp: Option<super::port::Response>) {
        if let Some(resp) = resp {
            use super::port::Response::*;

            app.clear_last_err();
            app.send_req(app::Request::RefreshPorts);

            match resp {
                Connect((port_nb, port_name)) => {
                    app.send_req(app::Request::Connect((port_nb, port_name)));
                }
                Disconnect(port_nb) => {
                    app.send_req(app::Request::Disconnect(port_nb));
                }
                CheckingList => (), // only refresh ports & clear last_err
            }
        }
    }
}
