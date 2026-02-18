//! Client-side auth.

use anyhow::{bail, Result};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

use crate::shared::{ClientMsg, Framed_, ServerMsg};

pub struct Auth(Hmac<Sha256>);

impl Auth {
    pub fn new(secret: &str) -> Self {
        let key = Sha256::new().chain_update(secret).finalize();
        Self(Hmac::new_from_slice(&key).expect("hmac accepts any key size"))
    }

    fn answer(&self, challenge: &Uuid) -> String {
        let mut mac = self.0.clone();
        mac.update(challenge.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    pub async fn handshake<T: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: &mut Framed_<T>,
    ) -> Result<()> {
        match stream.recv_timeout::<ServerMsg>().await? {
            Some(ServerMsg::Challenge(c)) => {
                stream.send(ClientMsg::Authenticate(self.answer(&c))).await
            }
            _ => bail!("expected Challenge from server"),
        }
    }
}
