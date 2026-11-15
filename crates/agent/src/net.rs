use std::net::{IpAddr, SocketAddr};

use layon_proto::{HostInfo, Interface, Ipv4Net};
use netdev::interface::types::InterfaceType;

pub fn host_info(port: u16) -> HostInfo {
    HostInfo {
        name: gethostname::gethostname().to_string_lossy().into_owned(),
        os: std::env::consts::OS.to_owned(),
        port,
        interfaces: interfaces(),
    }
}

/// Addresses to put in the pairing code.
pub fn socket_addrs(port: u16) -> Vec<SocketAddr> {
    interfaces()
        .iter()
        .flat_map(|i| &i.ipv4)
        .map(|net| SocketAddr::new(IpAddr::V4(net.addr), port))
        .collect()
}

/// Physical interfaces that currently hold an IPv4 address: the ones a magic
/// packet can reach.
fn interfaces() -> Vec<Interface> {
    netdev::get_interfaces()
        .into_iter()
        .filter(|i| i.is_up() && !i.is_loopback() && i.is_physical() && !i.ipv4.is_empty())
        .filter_map(|i| {
            let mac = i.mac_addr?.address();
            if mac == "00:00:00:00:00:00" {
                return None;
            }
            Some(Interface {
                name: i.friendly_name.unwrap_or(i.name),
                mac,
                wireless: i.if_type == InterfaceType::Wireless80211,
                ipv4: i
                    .ipv4
                    .iter()
                    .map(|net| Ipv4Net {
                        addr: net.addr(),
                        broadcast: net.broadcast(),
                    })
                    .collect(),
            })
        })
        .collect()
}
