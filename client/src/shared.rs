//! Shared protocol — client copy.

use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::timeout;
use tokio_util::codec::{AnyDelimiterCodec, Framed, FramedParts};

pub const CONTROL_PORT: u16 = 12267;
pub const MAX_FRAME: usize = 512;
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMsg {
    Hello { subdomain: String, proto: Proto },
    Authenticate(String),
    Accept(uuid::Uuid),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMsg {
    Challenge(uuid::Uuid),
    Hello { public_port: u16 },
    Heartbeat,
    Connection(uuid::Uuid),
    Error(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Proto {
    Tcp,
    Http,
}

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
