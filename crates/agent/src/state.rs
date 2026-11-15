use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use iroh::SecretKey;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const PAIRING_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub paired_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Pairing {
    secret: String,
    expires_at: u64,
}

/// The files are the only channel between `layon-agent run` and the other
/// subcommands, so every read goes back to disk.
#[derive(Debug, Clone)]
pub struct State {
    dir: PathBuf,
}

impl State {
    pub fn open(dir: Option<PathBuf>) -> Result<Self> {
        let dir = match dir {
            Some(dir) => dir,
            None => dirs::data_local_dir()
                .context("no per-user data directory on this platform; pass --data-dir")?
                .join("layon")
                .join("agent"),
        };
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        Ok(Self { dir })
    }

    pub fn secret_key(&self) -> Result<SecretKey> {
        let path = self.dir.join("secret.key");
        if let Ok(hex) = fs::read_to_string(&path) {
            return hex
                .trim()
                .parse()
                .with_context(|| format!("{} is not a valid key", path.display()));
        }
        let key = SecretKey::generate();
        let hex: String = key.to_bytes().iter().map(|b| format!("{b:02x}")).collect();
        write_private(&path, hex.as_bytes())?;
        Ok(key)
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        Ok(read_json(&self.dir.join("devices.json"))?.unwrap_or_default())
    }

    pub fn is_paired(&self, id: &str) -> bool {
        self.devices()
            .is_ok_and(|devices| devices.iter().any(|d| d.id == id))
    }

    pub fn add_device(&self, id: String, name: String) -> Result<()> {
        let mut devices = self.devices()?;
        devices.retain(|d| d.id != id);
        devices.push(Device {
            id,
            name,
            paired_at: now(),
        });
        write_json(&self.dir.join("devices.json"), &devices)
    }

    /// Returns whether a device was removed. `id` may be a unique prefix.
    pub fn remove_device(&self, id: &str) -> Result<bool> {
        let mut devices = self.devices()?;
        let before = devices.len();
        devices.retain(|d| !d.id.starts_with(id));
        write_json(&self.dir.join("devices.json"), &devices)?;
        Ok(devices.len() != before)
    }

    pub fn start_pairing(&self) -> Result<String> {
        let secret: String = rand::random::<[u8; 16]>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let pairing = Pairing {
            secret: secret.clone(),
            expires_at: now() + PAIRING_TTL.as_secs(),
        };
        write_json(&self.dir.join("pairing.json"), &pairing)?;
        Ok(secret)
    }

    /// A secret is good for exactly one successful pairing.
    pub fn redeem_pairing(&self, secret: &str) -> bool {
        let path = self.dir.join("pairing.json");
        let Ok(Some(pairing)) = read_json::<Pairing>(&path) else {
            return false;
        };
        if pairing.expires_at < now() {
            let _ = fs::remove_file(&path);
            return false;
        }
        if !constant_time_eq(pairing.secret.as_bytes(), secret.as_bytes()) {
            return false;
        }
        let _ = fs::remove_file(&path);
        true
    }

    pub fn cancel_pairing(&self) {
        let _ = fs::remove_file(self.dir.join("pairing.json"));
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .with_context(|| format!("parsing {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_private(path, &serde_json::to_vec_pretty(value)?)
}

/// Write-then-rename, so the running agent never reads a half-written file.
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(&tmp)
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::io::Write::write_all(&mut file, bytes)?;
    drop(file);
    fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}
