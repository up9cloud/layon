//! Stand-in for the mobile app, for testing an agent from another computer.

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use layon_client::{Client, PairingTicket, PowerAction};

#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Where this device's key and paired hosts live.
    #[arg(long, global = true, env = "LAYON_CLIENT_FILE")]
    file: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Pair using the `layon://pair/...` text that `layon-agent pair` prints.
    Pair {
        ticket: PairingTicket,
        #[arg(long, default_value = "layon-cli")]
        name: String,
    },
    /// List paired hosts.
    Hosts,
    Info { host: Option<String> },
    Sleep { host: Option<String> },
    Hibernate { host: Option<String> },
    /// Send Wake-on-LAN packets, then wait for the host to answer.
    Wake { host: Option<String> },
    Forget { host: Option<String> },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let cli = Cli::parse();
    let file = match cli.file {
        Some(file) => file,
        None => dirs::data_local_dir()
            .context("no per-user data directory; pass --file")?
            .join("layon")
            .join("client.json"),
    };
    let mut client = Client::open(file).await?;
    let result = run(&mut client, cli.command).await;
    client.close().await;
    result
}

async fn run(client: &mut Client, command: Command) -> Result<()> {
    match command {
        Command::Pair { ticket, name } => {
            let host = client.pair(&ticket, &name).await?;
            println!("Paired with {}.", host.info.name);
        }
        Command::Hosts => {
            for host in client.hosts() {
                println!("{}  {}", host.id, host.info.name);
            }
        }
        Command::Info { host } => {
            let id = pick(client, host)?;
            println!("{}", serde_json::to_string_pretty(&client.info(&id).await?)?);
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
            println!("Magic packets sent, waiting for the host…");
            let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
            while client.info(&id).await.is_err() {
                if tokio::time::Instant::now() >= deadline {
                    bail!("host did not come back within a minute");
                }
                client.wake(&id).await?;
            }
            println!("Awake.");
        }
        Command::Forget { host } => {
            let id = pick(client, host)?;
            client.forget(&id)?;
        }
    }
    Ok(())
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
