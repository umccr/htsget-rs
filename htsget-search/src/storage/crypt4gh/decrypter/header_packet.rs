use crate::storage::crypt4gh::error::Error::JoinHandleError;
use crate::storage::crypt4gh::error::Result;
use crate::storage::crypt4gh::SenderPublicKey;
use bytes::Bytes;
use crypt4gh::header::{deconstruct_header_body, DecryptedHeaderPackets};
use crypt4gh::Keys;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::task::{spawn_blocking, JoinHandle};

pin_project! {
    pub struct HeaderPacketsDecrypter {
        #[pin]
        handle: JoinHandle<Result<DecryptedHeaderPackets>>
    }
}

impl HeaderPacketsDecrypter {
  pub fn new(
    header_packets: Vec<Bytes>,
    keys: Vec<Keys>,
    sender_pubkey: Option<SenderPublicKey>,
  ) -> Self {
    Self {
      handle: spawn_blocking(|| {
        HeaderPacketsDecrypter::decrypt(header_packets, keys, sender_pubkey)
      }),
    }
  }

  pub fn decrypt(
    header_packets: Vec<Bytes>,
    keys: Vec<Keys>,
    sender_pubkey: Option<SenderPublicKey>,
  ) -> Result<DecryptedHeaderPackets> {
    Ok(deconstruct_header_body(
      header_packets
        .into_iter()
        .map(|bytes| bytes.to_vec())
        .collect(),
      keys.as_slice(),
      &sender_pubkey.map(|pubkey| pubkey.into_inner()),
    )?)
  }
}

impl Future for HeaderPacketsDecrypter {
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
  async fn header_packet_decrypter() {
    let (recipient_private_key, sender_public_key, header_packets, _) =
      get_first_header_packet().await;

    let data = HeaderPacketsDecrypter::new(
      header_packets,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    )
    .await
    .unwrap();

    assert_first_header_packet(data);
  }
}
