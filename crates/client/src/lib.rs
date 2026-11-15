//! Mobile side of layon: pairing, power requests and Wake-on-LAN.
//!
//! UI-free on purpose, so the same code backs the iOS app and the `layon`
//! test CLI.

use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use iroh::{
    Endpoint, EndpointAddr, EndpointId, SecretKey,
    endpoint::{Connection, presets},
};
pub use layon_proto::{HostInfo, PairingTicket, PowerAction};
use layon_proto::{ALPN, MAX_MESSAGE_BYTES, Request, Response, WOL_PORT, magic_packet};
use serde::{Deserialize, Serialize};
use tracing::debug;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(feature = "mdns")]
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub info: HostInfo,
    /// Where it last answered. A hint only: the id is the identity.
    pub addrs: BTreeSet<SocketAddr>,
}

#[derive(Serialize, Deserialize)]
struct Store {
    secret_key: SecretKey,
    hosts: Vec<Host>,
}

pub struct Client {
    endpoint: Endpoint,
    store: Store,
    path: PathBuf,
}

impl Client {
    /// `path` holds this device's key and its paired hosts; it is created on
    /// first use.
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let store = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("parsing {}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Store {
                secret_key: SecretKey::generate(),
                hosts: Vec::new(),
            },
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        let endpoint = Endpoint::builder(presets::Minimal)
            .secret_key(store.secret_key.clone())
            .bind()
            .await?;
        let client = Self {
            endpoint,
            store,
            path,
        };
        client.save()?;
        Ok(client)
    }

    pub fn hosts(&self) -> &[Host] {
        &self.store.hosts
    }

    pub async fn pair(&mut self, ticket: &PairingTicket, device_name: &str) -> Result<Host> {
        let id: EndpointId = ticket.id.parse().context("bad id in pairing code")?;
        let addrs: BTreeSet<SocketAddr> = ticket.addrs.iter().copied().collect();
        let (connection, addrs) = self.connect(id, addrs).await?;
        let request = Request::Pair {
            secret: ticket.secret.clone(),
            device_name: device_name.to_owned(),
        };
        let info = expect_info(request_on(&connection, &request).await?)?;
        connection.close(0u32.into(), b"done");

        let host = Host {
            id: ticket.id.clone(),
            info,
            addrs,
        };
        self.store.hosts.retain(|h| h.id != host.id);
        self.store.hosts.push(host.clone());
        self.save()?;
        Ok(host)
    }

    pub fn forget(&mut self, id: &str) -> Result<()> {
        self.store.hosts.retain(|h| h.id != id);
        self.save()
    }

    /// Fresh addresses from outside, e.g. a Bonjour browse on iOS.
    pub fn set_addrs(&mut self, id: &str, addrs: impl IntoIterator<Item = SocketAddr>) -> Result<()> {
        self.host_mut(id)?.addrs = addrs.into_iter().collect();
        self.save()
    }

    pub async fn info(&mut self, id: &str) -> Result<HostInfo> {
        let connection = self.connect_host(id).await?;
        let info = self.refresh(id, &connection).await?;
        connection.close(0u32.into(), b"done");
        Ok(info)
    }

    pub async fn power(&mut self, id: &str, action: PowerAction) -> Result<()> {
        let connection = self.connect_host(id).await?;
        // Last chance to learn the MAC and subnet that waking it will need.
        self.refresh(id, &connection).await?;
        let response = request_on(&connection, &Request::Power { action }).await?;
        connection.close(0u32.into(), b"done");
        match response {
            Response::Ok => Ok(()),
            other => Err(unexpected(other)),
        }
    }

    /// Sends Wake-on-LAN magic packets. Success means they left this device,
    /// not that the machine woke.
    pub async fn wake(&self, id: &str) -> Result<()> {
        let host = self.host(id)?;
        let socket = tokio::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
        socket.set_broadcast(true)?;

        let mut sent = 0;
        let mut last_error = None;
        for round in 0..3 {
            if round > 0 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            for interface in &host.info.interfaces {
                let Some(packet) = magic_packet(&interface.mac) else {
                    continue;
                };
                // Unicast last: it only lands while the network still maps the
                // sleeping machine's address, but it is the one form iOS lets
                // through without the multicast entitlement.
                let targets = std::iter::once(Ipv4Addr::BROADCAST)
                    .chain(interface.ipv4.iter().map(|net| net.broadcast))
                    .chain(interface.ipv4.iter().map(|net| net.addr));
                for target in targets {
                    match socket.send_to(&packet, (target, WOL_PORT)).await {
                        Ok(_) => sent += 1,
                        Err(e) => {
                            debug!(%target, "magic packet not sent: {e}");
                            last_error = Some(e);
                        }
                    }
                }
            }
        }
        match (sent, last_error) {
            (0, Some(e)) => Err(e).context("no magic packet could be sent"),
            (0, None) => bail!("no network interface is known for this host yet"),
            _ => Ok(()),
        }
    }

    pub async fn close(self) {
        self.endpoint.close().await;
    }

    fn host(&self, id: &str) -> Result<&Host> {
        self.store
            .hosts
            .iter()
            .find(|h| h.id == id)
            .ok_or_else(|| anyhow!("not paired with {id}"))
    }

    fn host_mut(&mut self, id: &str) -> Result<&mut Host> {
        self.store
            .hosts
            .iter_mut()
            .find(|h| h.id == id)
            .ok_or_else(|| anyhow!("not paired with {id}"))
    }

    async fn connect_host(&mut self, id: &str) -> Result<Connection> {
        let host = self.host(id)?;
        let mut addrs = host.addrs.clone();
        addrs.extend(info_addrs(&host.info));
        let (connection, addrs) = self.connect(id.parse()?, addrs).await?;
        let host = self.host_mut(id)?;
        if host.addrs != addrs {
            host.addrs = addrs;
            self.save()?;
        }
        Ok(connection)
    }

    /// Tries the known addresses, then looks the id up on the local network.
    /// Returns the address set that worked.
    async fn connect(
        &self,
        id: EndpointId,
        addrs: BTreeSet<SocketAddr>,
    ) -> Result<(Connection, BTreeSet<SocketAddr>)> {
        let error = match self.connect_addrs(id, &addrs).await {
            Ok(connection) => return Ok((connection, addrs)),
            Err(e) => e,
        };
        #[cfg(feature = "mdns")]
        {
            debug!("cached addresses failed ({error:#}), browsing mDNS");
            let found = discover(id.to_string()).await?;
            if !found.is_empty() {
                return Ok((self.connect_addrs(id, &found).await?, found));
            }
        }
        Err(error)
    }

    async fn connect_addrs(&self, id: EndpointId, addrs: &BTreeSet<SocketAddr>) -> Result<Connection> {
        if addrs.is_empty() {
            bail!("no known address");
        }
        let mut addr = EndpointAddr::new(id);
        for socket_addr in addrs {
            addr = addr.with_ip_addr(*socket_addr);
        }
        match tokio::time::timeout(CONNECT_TIMEOUT, self.endpoint.connect(addr, ALPN)).await {
            Ok(connection) => Ok(connection?),
            Err(_) => bail!("host did not answer (asleep, or on another network)"),
        }
    }

    async fn refresh(&mut self, id: &str, connection: &Connection) -> Result<HostInfo> {
        let info = expect_info(request_on(connection, &Request::Info).await?)?;
        let host = self.host_mut(id)?;
        if host.info != info {
            host.info = info.clone();
            self.save()?;
        }
        Ok(info)
    }

    fn save(&self) -> Result<()> {
        write_private(&self.path, &serde_json::to_vec_pretty(&self.store)?)
    }
}

fn info_addrs(info: &HostInfo) -> impl Iterator<Item = SocketAddr> + '_ {
    info.interfaces
        .iter()
        .flat_map(|i| &i.ipv4)
        .map(|net| SocketAddr::new(IpAddr::V4(net.addr), info.port))
}

async fn request_on(connection: &Connection, request: &Request) -> Result<Response> {
    let exchange = async {
        let (mut send, mut recv) = connection.open_bi().await?;
        send.write_all(&serde_json::to_vec(request)?).await?;
        send.finish()?;
        let bytes = recv.read_to_end(MAX_MESSAGE_BYTES).await?;
        anyhow::Ok(serde_json::from_slice::<Response>(&bytes)?)
    };
    match tokio::time::timeout(REQUEST_TIMEOUT, exchange).await {
        Ok(response) => response,
        Err(_) => bail!("host stopped answering"),
    }
}

fn expect_info(response: Response) -> Result<HostInfo> {
    match response {
        Response::Info(info) => Ok(info),
        other => Err(unexpected(other)),
    }
}

fn unexpected(response: Response) -> anyhow::Error {
    match response {
        Response::Error { message, .. } => anyhow!(message),
        other => anyhow!("unexpected response: {other:?}"),
    }
}

#[cfg(feature = "mdns")]
async fn discover(id: String) -> Result<BTreeSet<SocketAddr>> {
    use layon_proto::{SERVICE_TYPE, TXT_ID};
    use mdns_sd::{ServiceDaemon, ServiceEvent};

    tokio::task::spawn_blocking(move || {
        let daemon = ServiceDaemon::new()?;
        let events = daemon.browse(SERVICE_TYPE)?;
        let deadline = std::time::Instant::now() + DISCOVERY_TIMEOUT;
        let mut found = BTreeSet::new();
        while let Ok(event) = events.recv_deadline(deadline) {
            if let ServiceEvent::ServiceResolved(service) = event
                && service.get_property_val_str(TXT_ID) == Some(id.as_str())
            {
                found.extend(
                    service
                        .get_addresses_v4()
                        .into_iter()
                        .map(|ip| SocketAddr::new(IpAddr::V4(ip), service.get_port())),
                );
                break;
            }
        }
        let _ = daemon.shutdown();
        Ok(found)
    })
    .await?
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&tmp)?;
    std::io::Write::write_all(&mut file, bytes)?;
    drop(file);
    std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}
