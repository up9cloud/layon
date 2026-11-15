mod net;
mod power;
mod remote;
mod server;
mod state;

use std::{path::PathBuf, process::ExitCode, time::Duration};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use layon_proto::{DEFAULT_PORT, PairingTicket};
use qrcode::{QrCode, render::unicode::Dense1x2};

use crate::state::{PAIRING_TTL, State};

/// On the work machine, serves paired devices; `layon remote` drives one from
/// elsewhere.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Where keys and pairings live.
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
    /// Control a paired work machine from this one.
    Remote(remote::Args),
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,layon=info".into()),
        )
        // stdout is reserved for `remote --json`.
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let json = matches!(&cli.command, Some(Command::Remote(args)) if args.json);
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if json {
                eprintln!("{}", serde_json::json!({ "error": format!("{e:#}") }));
            } else {
                eprintln!("Error: {e:#}");
            }
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let data_dir = match cli.data_dir {
        Some(dir) => dir,
        None => dirs::data_local_dir()
            .context("no per-user data directory on this platform; pass --data-dir")?
            .join("layon"),
    };
    let command = cli.command.unwrap_or(Command::Run { dry_run: false });
    if let Command::Remote(args) = command {
        return remote::run(&data_dir, args).await;
    }
    let state = State::open(&data_dir)?;
    match command {
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
        Command::Remote(_) => unreachable!(),
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
    println!("{qr}\n\n{ticket}\n\nScan with the layon app, or pass the text to `layon remote pair`. Valid for {} minutes.", PAIRING_TTL.as_secs() / 60);

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
