//! sshx — expose a local port through an sshx-server tunnel.
//!
//! Usage:
//!   sshx -s myapp -p 3000              # HTTP tunnel
//!   sshx -s myssh -p 22 --tcp          # TCP/SSH tunnel
//!   sshx -s myssh -p 22 --tcp --secret mypassword

mod auth;
mod shared;

use std::sync::Arc;

use anyhow::{bail, Result};
use auth::Auth;
use clap::Parser;
use shared::{ClientMsg, Framed_, Proto, ServerMsg, CONTROL_PORT};
use tokio::{
    io::AsyncWriteExt,
    net::TcpStream,
    time::{sleep, Duration},
};
use tracing::{error, info, warn};
use uuid::Uuid;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Clone)]
#[command(name = "sshx", about = "Expose a local port through sshx tunnel")]
struct Cli {
    /// Subdomain to register (e.g. "myapp" → myapp.yourdomain.com).
    #[arg(short, long)]
    subdomain: String,

    /// Local port to expose.
    #[arg(short, long)]
    port: u16,

    /// Local host to forward traffic to.
    #[arg(long, default_value = "localhost")]
    host: String,

    /// sshx server address.
    #[arg(long, short = 'r', env = "SSHX_SERVER", default_value = "teamxpirates.qzz.io")]
    server: String,

    /// Use raw TCP mode (for SSH, databases, etc.). Default is HTTP.
    #[arg(long)]
    tcp: bool,

    /// Optional shared secret (must match server's --secret).
    #[arg(long, env = "SSHX_SECRET", hide_env_values = true)]
    secret: Option<String>,

    /// Automatically reconnect on disconnect.
    #[arg(long, default_value_t = true)]
    reconnect: bool,
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let proto = if cli.tcp { Proto::Tcp } else { Proto::Http };

    info!(
        subdomain = %cli.subdomain,
        port = cli.port,
        server = %cli.server,
        "starting sshx"
    );

    loop {
        match run(&cli, proto).await {
            Ok(_) => {
                info!("tunnel closed cleanly");
                break;
            }
            Err(e) => {
                error!(err = %e, "tunnel error");
                if !cli.reconnect {
                    return Err(e);
                }
                warn!("reconnecting in 3 seconds…");
                sleep(Duration::from_secs(3)).await;
            }
        }
    }
    Ok(())
}

// ── Main tunnel loop ──────────────────────────────────────────────────────────

async fn run(cli: &Cli, proto: Proto) -> Result<()> {
    // Open control connection.
    let stream = connect(&cli.server, CONTROL_PORT).await?;
    let mut ctrl = Framed_::new(stream);

    // Auth (if secret provided).
    if let Some(secret) = &cli.secret {
        Auth::new(secret).handshake(&mut ctrl).await?;
    }

    // Register subdomain.
    ctrl.send(ClientMsg::Hello {
        subdomain: cli.subdomain.clone(),
        proto,
    })
    .await?;

    // Read server Hello.
    let public_port = match ctrl.recv_timeout::<ServerMsg>().await? {
        Some(ServerMsg::Hello { public_port }) => public_port,
        Some(ServerMsg::Error(e)) => bail!("server error: {e}"),
        Some(ServerMsg::Challenge(_)) => bail!("server requires auth but no --secret given"),
        _ => bail!("unexpected response from server"),
    };

    println!();
    println!("  ✓  Tunnel active!");
    println!("     Subdomain : {}.{}", cli.subdomain, cli.server);
    println!("     Public    : {}:{}", cli.server, public_port);
    println!("     Local     : {}:{}", cli.host, cli.port);
    println!("     Protocol  : {:?}", proto);
    println!();

    // Share CLI config across spawned tasks.
    let cli = Arc::new(cli.clone());

    // Event loop.
    loop {
        match ctrl.recv::<ServerMsg>().await? {
            Some(ServerMsg::Heartbeat) => {}
            Some(ServerMsg::Connection(id)) => {
                let cli = Arc::clone(&cli);
                tokio::spawn(async move {
                    if let Err(e) = handle_data_connection(id, &cli).await {
                        warn!(err = %e, "data connection error");
                    }
                });
            }
            Some(ServerMsg::Error(e)) => error!("server: {e}"),
            None => break,
            _ => {}
        }
    }
    Ok(())
}

// ── Data connection (one per inbound TCP connection) ──────────────────────────

async fn handle_data_connection(id: Uuid, cli: &Cli) -> Result<()> {
    // Open a NEW control-port connection just for this data stream.
    let stream = connect(&cli.server, CONTROL_PORT).await?;
    let mut data_conn = Framed_::new(stream);

    // Re-auth if needed.
    if let Some(secret) = &cli.secret {
        Auth::new(secret).handshake(&mut data_conn).await?;
    }

    // Tell server which pending connection we're accepting.
    data_conn.send(ClientMsg::Accept(id)).await?;

    // Connect to local service.
    let mut local = connect(&cli.host, cli.port).await?;

    // Upgrade: discard the framing codec, use raw TCP from here.
    let mut parts = data_conn.into_parts();
    local.write_all(&parts.read_buf).await?;
    tokio::io::copy_bidirectional(&mut local, &mut parts.io).await?;
    Ok(())
}

// ── Helper ────────────────────────────────────────────────────────────────────

async fn connect(host: &str, port: u16) -> Result<TcpStream> {
    TcpStream::connect((host, port))
        .await
        .map_err(|e| anyhow::anyhow!("cannot connect to {}:{} — {}", host, port, e))
}
