//! Wire types shared by the agent (work machine) and the client (phone or tablet).
//!
//! One request per QUIC bidirectional stream: the client writes a JSON
//! [`Request`] and finishes the stream, the agent answers with a JSON
//! [`Response`]. Identity and encryption come from the transport, so nothing
//! here carries keys or signatures.

use std::{
    fmt,
    net::{Ipv4Addr, SocketAddr},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

pub const ALPN: &[u8] = b"layon/1";
pub const DEFAULT_PORT: u16 = 47126;
/// DNS-SD service type the agent advertises; the TXT record `id` holds its
/// endpoint id.
pub const SERVICE_TYPE: &str = "_layon._udp.local.";
pub const TXT_ID: &str = "id";
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const WOL_PORT: u16 = 9;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// The only request an unpaired device may send.
    Pair { secret: String, device_name: String },
    Info,
    Power { action: PowerAction },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerAction {
    Sleep,
    Hibernate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Info(HostInfo),
    Error { code: ErrorCode, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotPaired,
    BadPairingSecret,
    BadRequest,
    Failed,
}

/// Everything the mobile device must remember to find and wake the machine later.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostInfo {
    pub name: String,
    pub os: String,
    pub port: u16,
    pub interfaces: Vec<Interface>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interface {
    pub name: String,
    /// `aa:bb:cc:dd:ee:ff`
    pub mac: String,
    pub wireless: bool,
    pub ipv4: Vec<Ipv4Net>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ipv4Net {
    pub addr: Ipv4Addr,
    pub broadcast: Ipv4Addr,
}

/// What the pairing QR code encodes:
/// `layon://pair/<endpoint id>/<secret>/<addr>,<addr>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingTicket {
    pub id: String,
    pub secret: String,
    pub addrs: Vec<SocketAddr>,
}

const TICKET_PREFIX: &str = "layon://pair/";

impl fmt::Display for PairingTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let addrs: Vec<String> = self.addrs.iter().map(ToString::to_string).collect();
        write!(
            f,
            "{TICKET_PREFIX}{}/{}/{}",
            self.id,
            self.secret,
            addrs.join(",")
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidTicket;

impl fmt::Display for InvalidTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a layon pairing code")
    }
}

impl std::error::Error for InvalidTicket {}

impl FromStr for PairingTicket {
    type Err = InvalidTicket;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rest = s.trim().strip_prefix(TICKET_PREFIX).ok_or(InvalidTicket)?;
        let mut parts = rest.split('/');
        let (Some(id), Some(secret), Some(addrs), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(InvalidTicket);
        };
        if id.is_empty() || secret.is_empty() {
            return Err(InvalidTicket);
        }
        let addrs = addrs
            .split(',')
            .filter(|a| !a.is_empty())
            .map(|a| a.parse().map_err(|_| InvalidTicket))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            id: id.to_owned(),
            secret: secret.to_owned(),
            addrs,
        })
    }
}

/// 6 × `0xff` followed by the MAC 16 times.
pub fn magic_packet(mac: &str) -> Option<[u8; 102]> {
    let mut bytes = [0u8; 6];
    let mut parts = mac.split([':', '-']);
    for b in &mut bytes {
        *b = u8::from_str_radix(parts.next()?, 16).ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    let mut packet = [0xffu8; 102];
    for chunk in packet[6..].as_chunks_mut::<6>().0 {
        *chunk = bytes;
    }
    Some(packet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_round_trips() {
        let ticket = PairingTicket {
            id: "abc123".into(),
            secret: "s3cret".into(),
            addrs: vec![
                "192.168.1.5:47126".parse().unwrap(),
                "[fe80::1]:47126".parse().unwrap(),
            ],
        };
        assert_eq!(ticket.to_string().parse::<PairingTicket>(), Ok(ticket));
    }

    #[test]
    fn ticket_rejects_garbage() {
        for s in ["", "https://example.com", "layon://pair/id", "layon://pair/id/s/nope"] {
            assert_eq!(s.parse::<PairingTicket>(), Err(InvalidTicket), "{s}");
        }
    }

    #[test]
    fn magic_packet_layout() {
        let packet = magic_packet("AA-bb:0c:01:02:03").unwrap();
        assert_eq!(packet[..6], [0xff; 6]);
        assert_eq!(packet[6..12], [0xaa, 0xbb, 0x0c, 1, 2, 3]);
        assert_eq!(packet[96..], [0xaa, 0xbb, 0x0c, 1, 2, 3]);
        assert!(magic_packet("aa:bb").is_none());
        assert!(magic_packet("aa:bb:cc:dd:ee:ff:00").is_none());
    }
}
