use std::{collections::HashMap, sync::Mutex, time::Duration};

use mdns_sd::{ServiceDaemon, ServiceInfo};

use crate::util::lock;

const SERVICE_TYPE: &str = "_http._tcp.local.";

/// Makes the help address resolve on the LAN. A browser cannot browse Bonjour, so the service
/// record exists only to carry the name; nothing is published outside a help request.
pub struct Bonjour {
    daemon: Option<ServiceDaemon>,
    published: Mutex<Option<String>>,
}

impl Bonjour {
    pub fn new() -> Self {
        let daemon = match ServiceDaemon::new() {
            Ok(daemon) => Some(daemon),
            Err(error) => {
                eprintln!("Bonjour is unavailable: {error}");
                None
            }
        };
        Self { daemon, published: Mutex::new(None) }
    }

    /// Answers for `name` (such as `assist-maple-river.local`) until `unpublish`. Returns whether it worked.
    pub fn publish(&self, name: &str, port: u16) -> bool {
        let Some(daemon) = &self.daemon else { return false };
        let mut published = lock(&self.published);
        if published.is_some() {
            return true;
        }

        let instance = format!("Light Assist ({})", name.trim_end_matches(".local"));
        let txt = HashMap::from([("path".to_owned(), "/".to_owned())]);
        let registered = ServiceInfo::new(SERVICE_TYPE, &instance, &format!("{name}."), (), port, txt).and_then(|info| {
            let info = info.enable_addr_auto();
            let fullname = info.get_fullname().to_owned();
            daemon.register(info).map(|()| fullname)
        });

        match registered {
            Ok(fullname) => {
                *published = Some(fullname);
                true
            }
            Err(error) => {
                eprintln!("Could not publish the help address: {error}");
                false
            }
        }
    }

    pub fn unpublish(&self) {
        let Some(daemon) = &self.daemon else { return };
        if let Some(fullname) = lock(&self.published).take() {
            let _ = daemon.unregister(&fullname);
        }
    }

    pub fn close(&self) {
        let Some(daemon) = &self.daemon else { return };

        if let Some(fullname) = lock(&self.published).take() {
            if let Ok(done) = daemon.unregister(&fullname) {
                // Gives the goodbye packets time to leave.
                let _ = done.recv_timeout(Duration::from_millis(500));
            }
        }
        let _ = daemon.shutdown();
    }
}
