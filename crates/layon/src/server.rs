use std::{
    net::{Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use anyhow::{Context, Result};
use iroh::{
    Endpoint,
    endpoint::{Connection, presets},
};
use layon_proto::{
    ALPN, ErrorCode, MAX_MESSAGE_BYTES, PowerAction, Request, Response, SERVICE_TYPE, TXT_ID,
};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use tracing::{info, warn};

use crate::{net, power, state::State};

/// Long enough for the reply to reach the mobile device before the NIC goes down.
const POWER_DELAY: Duration = Duration::from_secs(1);

pub async fn run(state: State, port: u16, dry_run: bool) -> Result<()> {
    let endpoint = Endpoint::builder(presets::Minimal)
        .secret_key(state.secret_key()?)
        .alpns(vec![ALPN.to_vec()])
        .clear_ip_transports()
        .bind_addr((Ipv4Addr::UNSPECIFIED, port))?
        .bind_addr((Ipv6Addr::UNSPECIFIED, port))?
        .bind()
        .await
        .with_context(|| format!("binding UDP port {port}"))?;
    let id = endpoint.id().to_string();
    info!(%id, port, "listening");

    // Dropping the daemon withdraws the advertisement.
    let _mdns = advertise(&id, port)
        .inspect_err(|e| warn!("mDNS advertisement failed, clients must rely on cached addresses: {e:#}"))
        .ok();

    loop {
        tokio::select! {
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break };
                let state = state.clone();
                tokio::spawn(async move {
                    match incoming.await {
                        Ok(connection) => serve(connection, state, port, dry_run).await,
                        Err(e) => warn!("handshake failed: {e:#}"),
                    }
                });
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    endpoint.close().await;
    Ok(())
}

fn advertise(id: &str, port: u16) -> Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;
    // DNS labels cap at 63 bytes, one short of a full id.
    let instance = &id[..16];
    let service = ServiceInfo::new(
        SERVICE_TYPE,
        instance,
        &format!("layon-{instance}.local."),
        "",
        port,
        &[(TXT_ID, id)][..],
    )?
    .enable_addr_auto();
    daemon.register(service)?;
    Ok(daemon)
}

async fn serve(connection: Connection, state: State, port: u16, dry_run: bool) {
    let remote = connection.remote_id().to_string();
    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
        let request = match recv.read_to_end(MAX_MESSAGE_BYTES).await {
            Ok(bytes) => serde_json::from_slice::<Request>(&bytes),
            Err(e) => {
                warn!(%remote, "reading request: {e:#}");
                return;
            }
        };
        let (response, power) = match request {
            Ok(request) => handle(&state, &remote, port, request),
            Err(e) => (error(ErrorCode::BadRequest, e), None),
        };
        let bytes = serde_json::to_vec(&response).expect("responses always serialize");
        if send.write_all(&bytes).await.is_err() || send.finish().is_err() {
            return;
        }
        if let Some(action) = power {
            info!(%remote, ?action, dry_run, "power");
            if dry_run {
                continue;
            }
            tokio::spawn(async move {
                tokio::time::sleep(POWER_DELAY).await;
                // Blocks until the machine resumes.
                let result = tokio::task::spawn_blocking(move || power::apply(action)).await;
                if let Ok(Err(e)) = result {
                    warn!(?action, "power action failed: {e:#}");
                }
            });
        }
    }
}

fn handle(
    state: &State,
    remote: &str,
    port: u16,
    request: Request,
) -> (Response, Option<PowerAction>) {
    if let Request::Pair {
        secret,
        device_name,
    } = request
    {
        if !state.redeem_pairing(&secret) {
            warn!(%remote, "pairing rejected");
            return (
                error(ErrorCode::BadPairingSecret, "pairing code is wrong or expired"),
                None,
            );
        }
        return match state.add_device(remote.to_owned(), device_name.clone()) {
            Ok(()) => {
                info!(%remote, device_name, "paired");
                (Response::Info(net::host_info(port)), None)
            }
            Err(e) => (error(ErrorCode::Failed, format!("{e:#}")), None),
        };
    }

    if !state.is_paired(remote) {
        return (error(ErrorCode::NotPaired, "this device is not paired"), None);
    }
    match request {
        Request::Pair { .. } => unreachable!("handled above"),
        Request::Info => (Response::Info(net::host_info(port)), None),
        Request::Power { action } => (Response::Ok, Some(action)),
    }
}

fn error(code: ErrorCode, message: impl ToString) -> Response {
    Response::Error {
        code,
        message: message.to_string(),
    }
}
