//! `layon remote`: drive a paired work machine. The apps run these commands
//! with `--json` (iOS excepted, which links `layon-client` directly).

use std::{path::Path, time::Duration};

use anyhow::{Result, bail};
use clap::Subcommand;
use layon_client::{Client, Host, PairingTicket, PowerAction};
use serde_json::json;

#[derive(clap::Args)]
pub struct Args {
    /// Machine-readable output: `pair`, `hosts` and `info` print JSON; a
    /// failure exits 1 with `{"error": ...}` on stderr.
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Pair using the `layon://pair/...` text that `layon pair` prints.
    Pair {
        ticket: PairingTicket,
        /// How the host lists this device; defaults to this machine's name.
        #[arg(long)]
        name: Option<String>,
    },
    /// List paired hosts.
    Hosts,
    /// Show what the host reports about itself; fails if it is not awake.
    Info { host: Option<String> },
    /// Put the host to sleep.
    Sleep { host: Option<String> },
    /// Hibernate the host.
    Hibernate { host: Option<String> },
    /// Send Wake-on-LAN packets, then wait for the host to answer.
    Wake { host: Option<String> },
    /// Remove the pairing from this device.
    Forget { host: Option<String> },
}

pub async fn run(data_dir: &Path, args: Args) -> Result<()> {
    let mut client = Client::open(data_dir.join("client.json")).await?;
    let result = execute(&mut client, args.command, args.json).await;
    client.close().await;
    result
}

async fn execute(client: &mut Client, command: Command, json: bool) -> Result<()> {
    match command {
        Command::Pair { ticket, name } => {
            let name = name.unwrap_or_else(|| gethostname::gethostname().to_string_lossy().into_owned());
            let host = client.pair(&ticket, &name).await?;
            if json {
                println!("{}", host_json(&host));
            } else {
                println!("Paired with {}.", host.info.name);
            }
        }
        Command::Hosts => {
            if json {
                let hosts: Vec<_> = client.hosts().iter().map(host_json).collect();
                println!("{}", serde_json::Value::Array(hosts));
            } else {
                for host in client.hosts() {
                    println!("{}  {}", host.id, host.info.name);
                }
            }
        }
        Command::Info { host } => {
            let id = pick(client, host)?;
            let info = client.info(&id).await?;
            if json {
                println!("{}", serde_json::to_string(&info)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&info)?);
            }
        }
        Command::Sleep { host } => {
            let id = pick(client, host)?;
            client.power(&id, PowerAction::Sleep).await?;
        }
        Command::Hibernate { host } => {
            let id = pick(client, host)?;
            client.power(&id, PowerAction::Hibernate).await?;
        }
        Command::Wake { host } => {
            let id = pick(client, host)?;
            client.wake(&id).await?;
            if !json {
                println!("Magic packets sent, waiting for the host…");
            }
            let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
            while client.info(&id).await.is_err() {
                if tokio::time::Instant::now() >= deadline {
                    bail!("host did not come back within a minute");
                }
                client.wake(&id).await?;
            }
            if !json {
                println!("Awake.");
            }
        }
        Command::Forget { host } => {
            let id = pick(client, host)?;
            client.forget(&id)?;
        }
    }
    Ok(())
}

fn host_json(host: &Host) -> serde_json::Value {
    json!({ "id": host.id, "name": host.info.name, "os": host.info.os })
}

/// Matches a host by name or id prefix; with none given, the only paired host.
fn pick(client: &Client, wanted: Option<String>) -> Result<String> {
    let hosts = client.hosts();
    let matches: Vec<_> = match &wanted {
        Some(wanted) => hosts
            .iter()
            .filter(|h| h.info.name == *wanted || h.id.starts_with(wanted.as_str()))
            .collect(),
        None => hosts.iter().collect(),
    };
    match matches.as_slice() {
        [host] => Ok(host.id.clone()),
        [] => bail!("no matching paired host"),
        _ => bail!("several hosts match; name one"),
    }
}
