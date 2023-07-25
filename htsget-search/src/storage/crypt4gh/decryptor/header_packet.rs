use crate::storage::crypt4gh::error::Error::JoinHandleError;
use crate::storage::crypt4gh::error::Result;
use crate::storage::crypt4gh::SenderPublicKey;
use bytes::Bytes;
use crypt4gh::header::{deconstruct_header_body, DecryptedHeaderPackets};
use crypt4gh::Keys;
use futures::Stream;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::task::JoinHandle;

pin_project! {
    pub struct HeaderPacketDecryptor {
        #[pin]
        handle: JoinHandle<Result<DecryptedHeaderPackets>>
    }
}

impl HeaderPacketDecryptor {
  pub fn new(
    header_packet: Bytes,
    keys: Vec<Keys>,
    sender_pubkey: Option<SenderPublicKey>,
  ) -> Self {
    Self {
      handle: tokio::task::spawn_blocking(move || {
        HeaderPacketDecryptor::decrypt(header_packet, keys, sender_pubkey)
      }),
    }
  }

  pub fn decrypt(
    header_packet: Bytes,
    keys: Vec<Keys>,
    sender_pubkey: Option<SenderPublicKey>,
  ) -> Result<DecryptedHeaderPackets> {
    Ok(deconstruct_header_body(
      vec![header_packet.to_vec()],
      keys.as_slice(),
      &sender_pubkey.map(|pubkey| pubkey.into_inner()),
    )?)
  }
}

impl Future for HeaderPacketDecryptor {
  type Output = Result<DecryptedHeaderPackets>;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    self.project().handle.poll(cx).map_err(JoinHandleError)?
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::storage::crypt4gh::decoder::tests::{
    assert_first_header_packet, get_first_header_packet,
  };

  #[tokio::test]
  async fn header_packet_decryptor() {
    let (recipient_private_key, sender_public_key, header_packet, _) =
      get_first_header_packet().await;

    let data = HeaderPacketDecryptor::new(
      header_packet,
      vec![recipient_private_key],
      Some(SenderPublicKey {
        bytes: sender_public_key,
      }),
    )
    .await
    .unwrap();

    assert_first_header_packet(data);
  }
}
