//! Shared protocol definitions for sshx tunnel.
//!
//! Control plane: null-delimited JSON on port 7835.
//! Data plane:   raw TCP copy_bidirectional.

use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::timeout;
use tokio_util::codec::{AnyDelimiterCodec, Framed, FramedParts};

/// Control port — clients connect here first.
pub const CONTROL_PORT: u16 = 7835;

/// Max JSON frame size (bytes).
pub const MAX_FRAME: usize = 512;

/// Timeout for initial handshake messages.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

// ── Messages: Client → Server ────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Step 1 after optional auth: register a subdomain + protocol.
    Hello {
        subdomain: String,
        proto: Proto,
    },
    /// Auth challenge response.
    Authenticate(String),
    /// Accept a pending proxied connection.
    Accept(uuid::Uuid),
}

// ── Messages: Server → Client ────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMsg {
    /// Auth challenge (only sent when server has a secret).
    Challenge(uuid::Uuid),
    /// Subdomain registered OK. `public_port` is the exposed port on the server.
    Hello { public_port: u16 },
    /// Keepalive — sent every ~500 ms on idle control connections.
    Heartbeat,
    /// A new inbound connection arrived; client should open a data connection.
    Connection(uuid::Uuid),
    /// Something went wrong.
    Error(String),
}

// ── Protocol type ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Proto {
    Tcp,
    Http,
}

// ── Framed transport ──────────────────────────────────────────────────────────

/// Null-delimited JSON transport.
pub struct Framed_<U>(Framed<U, AnyDelimiterCodec>);

impl<U: AsyncRead + AsyncWrite + Unpin> Framed_<U> {
    pub fn new(stream: U) -> Self {
        let codec = AnyDelimiterCodec::new_with_max_length(vec![0], vec![0], MAX_FRAME);
        Self(Framed::new(stream, codec))
    }

    pub async fn recv<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
        match self.0.next().await {
            Some(Ok(bytes)) => Ok(serde_json::from_slice(&bytes).context("parse error")?),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    pub async fn recv_timeout<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
        timeout(HANDSHAKE_TIMEOUT, self.recv())
            .await
            .context("handshake timed out")?
    }

    pub async fn send<T: Serialize>(&mut self, msg: T) -> Result<()> {
        self.0.send(serde_json::to_string(&msg)?).await?;
        Ok(())
    }

    pub fn into_parts(self) -> FramedParts<U, AnyDelimiterCodec> {
        self.0.into_parts()
    }
}
