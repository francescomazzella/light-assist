use std::{
    net::Ipv4Addr,
    sync::{Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Tailscale gives each device an address in 100.64.0.0/10, reachable only inside its own tailnet.
pub fn is_tailscale(address: Ipv4Addr) -> bool {
    let octets = address.octets();
    octets[0] == 100 && octets[1] & 0xc0 == 64
}

/// Addresses that cannot belong to a host on the open internet: a LAN or a Tailscale network.
pub fn is_private_network(address: Ipv4Addr) -> bool {
    address.is_private() || address.is_link_local() || is_tailscale(address)
}

/// A panic elsewhere must not take the whole agent down with it.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
