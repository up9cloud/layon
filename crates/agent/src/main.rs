mod net;
mod power;
mod server;
mod state;

use std::{path::PathBuf, time::Duration};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use layon_proto::{DEFAULT_PORT, PairingTicket};
use qrcode::{QrCode, render::unicode::Dense1x2};

use crate::state::{PAIRING_TTL, State};

/// Work-machine side of layon.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Where the key and paired devices live.
    #[arg(long, global = true, env = "LAYON_DATA_DIR")]
    data_dir: Option<PathBuf>,
    /// UDP port to listen on.
    #[arg(long, global = true, env = "LAYON_PORT", default_value_t = DEFAULT_PORT)]
    port: u16,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Serve paired devices (the default).
    Run {
        /// Log power requests instead of carrying them out.
        #[arg(long)]
        dry_run: bool,
    },
    /// Show a QR code for a phone or tablet to scan. Needs `run` going at the same time.
    Pair,
    /// List paired devices.
    Devices,
    /// Forget a device, by id or unique id prefix.
    Unpair { id: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,layon_agent=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let state = State::open(cli.data_dir)?;
    match cli.command.unwrap_or(Command::Run { dry_run: false }) {
        Command::Run { dry_run } => server::run(state, cli.port, dry_run).await,
        Command::Pair => pair(&state, cli.port).await,
        Command::Devices => {
            for device in state.devices()? {
                println!("{}  {}", device.id, device.name);
            }
            Ok(())
        }
        Command::Unpair { id } => {
            if !state.remove_device(&id)? {
                bail!("no paired device with id {id}");
            }
            Ok(())
        }
    }
}

async fn pair(state: &State, port: u16) -> Result<()> {
    let known = state.devices()?.len();
    let ticket = PairingTicket {
        id: state.secret_key()?.public().to_string(),
        secret: state.start_pairing()?,
        addrs: net::socket_addrs(port),
    }
    .to_string();

    let qr = QrCode::new(&ticket)?
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .build();
    println!("{qr}\n\n{ticket}\n\nScan with the layon app. Valid for {} minutes.", PAIRING_TTL.as_secs() / 60);

    let deadline = tokio::time::Instant::now() + PAIRING_TTL;
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            _ = tokio::signal::ctrl_c() => break,
        }
        let devices = state.devices()?;
        if devices.len() > known
            && let Some(device) = devices.last()
        {
            println!("Paired with {}.", device.name);
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
    }
    state.cancel_pairing();
    bail!("no device paired");
}
