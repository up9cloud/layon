//! Swift/Kotlin face of `layon-client`. Bindings are generated from this file
//! by `uniffi-bindgen`; nothing here holds logic of its own.

use std::sync::Arc;

use layon_client::Client;
use tokio::sync::Mutex;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum LayonError {
    #[error("{0}")]
    Failed(String),
}

impl From<anyhow::Error> for LayonError {
    fn from(e: anyhow::Error) -> Self {
        Self::Failed(format!("{e:#}"))
    }
}

type Result<T> = std::result::Result<T, LayonError>;

#[derive(uniffi::Enum)]
pub enum PowerAction {
    Sleep,
    Hibernate,
}

#[derive(uniffi::Record)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub os: String,
}

impl From<&layon_client::Host> for Host {
    fn from(host: &layon_client::Host) -> Self {
        Self {
            id: host.id.clone(),
            name: host.info.name.clone(),
            os: host.info.os.clone(),
        }
    }
}

#[derive(uniffi::Object)]
pub struct LayonClient {
    inner: Mutex<Client>,
}

#[uniffi::export(async_runtime = "tokio")]
impl LayonClient {
    /// `path` is a file inside the app's own storage.
    #[uniffi::constructor]
    pub async fn open(path: String) -> Result<Arc<Self>> {
        let inner = Mutex::new(Client::open(path).await?);
        Ok(Arc::new(Self { inner }))
    }

    pub async fn hosts(&self) -> Vec<Host> {
        self.inner.lock().await.hosts().iter().map(Host::from).collect()
    }

    /// `ticket` is the text of the scanned QR code.
    pub async fn pair(&self, ticket: String, device_name: String) -> Result<Host> {
        let ticket = ticket.parse().map_err(anyhow::Error::from)?;
        let host = self.inner.lock().await.pair(&ticket, &device_name).await?;
        Ok(Host::from(&host))
    }

    pub async fn forget(&self, id: String) -> Result<()> {
        Ok(self.inner.lock().await.forget(&id)?)
    }

    /// `addrs` are `ip:port` strings from a Bonjour browse; unparsable ones
    /// are dropped.
    pub async fn set_addrs(&self, id: String, addrs: Vec<String>) -> Result<()> {
        let addrs = addrs.iter().filter_map(|a| a.parse().ok());
        Ok(self.inner.lock().await.set_addrs(&id, addrs)?)
    }

    /// Succeeds only if the host answers, so it doubles as "is it awake".
    pub async fn ping(&self, id: String) -> Result<()> {
        self.inner.lock().await.info(&id).await?;
        Ok(())
    }

    pub async fn power(&self, id: String, action: PowerAction) -> Result<()> {
        let action = match action {
            PowerAction::Sleep => layon_client::PowerAction::Sleep,
            PowerAction::Hibernate => layon_client::PowerAction::Hibernate,
        };
        Ok(self.inner.lock().await.power(&id, action).await?)
    }

    pub async fn wake(&self, id: String) -> Result<()> {
        Ok(self.inner.lock().await.wake(&id).await?)
    }
}
