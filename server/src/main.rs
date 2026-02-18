//! sshx-server — accepts client registrations and proxies inbound connections.

mod auth;
mod shared;

use std::{
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use auth::Auth;
use clap::Parser;
use dashmap::DashMap;
use shared::{ClientMsg, Framed_, Proto, ServerMsg, CONTROL_PORT};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    time::{sleep, timeout},
};
use tracing::{info, warn};
use uuid::Uuid;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "sshx-server", about = "sshx tunnel server")]
struct Cli {
    /// Secret clients must know (optional).
    #[arg(long, short, env = "SSHX_SECRET")]
    secret: Option<String>,

    /// Minimum port for tunnels.
    #[arg(long, default_value_t = 2000, env = "SSHX_MIN_PORT")]
    min_port: u16,

    /// Maximum port for tunnels.
    #[arg(long, default_value_t = 65000, env = "SSHX_MAX_PORT")]
    max_port: u16,

    /// Bind address.
    #[arg(long, default_value = "0.0.0.0", env = "SSHX_BIND")]
    bind: IpAddr,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct State {
    /// subdomain → port mapping (so names are unique).
    subdomains: DashMap<String, u16>,
    /// pending inbound connections waiting for client Accept.
    pending: DashMap<Uuid, TcpStream>,
    auth: Option<Auth>,
    min_port: u16,
    max_port: u16,
    bind: IpAddr,
}

impl State {
    fn new(min_port: u16, max_port: u16, bind: IpAddr, secret: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            subdomains: DashMap::new(),
            pending: DashMap::new(),
            auth: secret.map(Auth::new),
            min_port,
            max_port,
            bind,
        })
    }

    /// Try to bind a listener for the given subdomain.
    async fn claim_port(&self, subdomain: &str, _proto: Proto) -> Result<TcpListener, String> {
        if self.subdomains.contains_key(subdomain) {
            return Err(format!("subdomain '{}' is already taken", subdomain));
        }
        // Try 150 random ports (same probabilistic argument as bore).
        for _ in 0..150 {
            let port = fastrand::u16(self.min_port..=self.max_port);
            match TcpListener::bind((self.bind, port)).await {
                Ok(l) => {
                    self.subdomains.insert(subdomain.to_owned(), port);
                    return Ok(l);
                }
                Err(_) => continue,
            }
        }
        Err("no free ports available".into())
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    let state = State::new(cli.min_port, cli.max_port, cli.bind, cli.secret.as_deref());
    let listener = TcpListener::bind((cli.bind, CONTROL_PORT)).await?;
    info!(addr = %cli.bind, port = CONTROL_PORT, "sshx-server listening");

    loop {
        let (stream, addr) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = handle_control(stream, state).await {
                warn!(%addr, err = %e, "connection error");
            }
        });
    }
}

// ── Control connection handler ────────────────────────────────────────────────

async fn handle_control(stream: TcpStream, state: Arc<State>) -> Result<()> {
    let mut ctrl = Framed_::new(stream);

    // Auth (optional).
    if let Some(auth) = &state.auth {
        if let Err(e) = auth.handshake_server(&mut ctrl).await {
            ctrl.send(ServerMsg::Error(e.to_string())).await?;
            return Ok(());
        }
    }

    // First real message from client.
    match ctrl.recv_timeout::<ClientMsg>().await? {
        // ── Register a tunnel ──────────────────────────────────────────────
        Some(ClientMsg::Hello { subdomain, proto }) => {
            let listener = match state.claim_port(&subdomain, proto).await {
                Ok(l) => l,
                Err(e) => {
                    ctrl.send(ServerMsg::Error(e)).await?;
                    return Ok(());
                }
            };
            let public_port = listener.local_addr()?.port();
            ctrl.send(ServerMsg::Hello { public_port }).await?;
            info!(subdomain, public_port, "tunnel registered");

            // Drive the tunnel: heartbeat + accept inbound connections.
            let result = drive_tunnel(ctrl, listener, &state, &subdomain).await;
            state.subdomains.remove(&subdomain);
            info!(subdomain, "tunnel closed");
            result
        }

        // ── Client is accepting a pending inbound connection ───────────────
        Some(ClientMsg::Accept(id)) => {
            match state.pending.remove(&id) {
                Some((_, mut inbound)) => {
                    let mut parts = ctrl.into_parts();
                    // Flush any buffered bytes first.
                    inbound.write_all(&parts.read_buf).await?;
                    tokio::io::copy_bidirectional(&mut inbound, &mut parts.io).await?;
                }
                None => warn!(%id, "Accept for unknown connection"),
            }
            Ok(())
        }

        _ => Ok(()),
    }
}

// ── Tunnel driver: heartbeat + forward inbound connections ────────────────────

async fn drive_tunnel(
    mut ctrl: Framed_<TcpStream>,
    listener: TcpListener,
    state: &Arc<State>,
    subdomain: &str,
) -> Result<()> {
    loop {
        // Send heartbeat; if client is gone, exit.
        if ctrl.send(ServerMsg::Heartbeat).await.is_err() {
            return Ok(());
        }

        // Wait up to 500 ms for a new inbound connection.
        match timeout(Duration::from_millis(500), listener.accept()).await {
            Ok(Ok((stream, addr))) => {
                let id = Uuid::new_v4();
                info!(%addr, %subdomain, "inbound connection");

                // Store it; clean up after 10 s if client never accepts.
                state.pending.insert(id, stream);
                let pending = Arc::clone(state);
                tokio::spawn(async move {
                    sleep(Duration::from_secs(10)).await;
                    if pending.pending.remove(&id).is_some() {
                        warn!(%id, "stale pending connection removed");
                    }
                });

                ctrl.send(ServerMsg::Connection(id)).await?;
            }
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {} // timeout — just loop and heartbeat again
        }
    }
}
