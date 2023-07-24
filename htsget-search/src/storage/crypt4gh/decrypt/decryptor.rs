use crate::storage::crypt4gh::decrypt::decoder::{Block, DecodedBlock};
use crate::storage::crypt4gh::error::Error::{Crypt4GHError, JoinHandleError};
use crate::storage::crypt4gh::error::Result;
use bytes::Bytes;
use crypt4gh::error::Crypt4GHError::NoSupportedEncryptionMethod;
use crypt4gh::header::{deconstruct_header_body, DecryptedHeaderPackets};
use crypt4gh::{body_decrypt, Keys, WriteInfo};
use futures::ready;
use futures::Stream;
use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::io::Cursor;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncRead;
use tokio::task::JoinHandle;
use tokio_util::codec::FramedRead;

#[derive(Debug, Clone)]
pub struct SenderPublicKey {
  bytes: Vec<u8>,
}

// Decrypts/encrypts one block?
pin_project! {
    struct Decryptor<R> {
        #[pin]
        inner: FramedRead<R, Block>,
        keys: Vec<Keys>,
        sender_pubkey: Option<SenderPublicKey>,
        session_keys: Vec<Vec<u8>>
    }
}

impl<R> Decryptor<R>
where
  R: AsyncRead,
{
  pub fn new(inner: R, keys: Vec<Keys>, sender_pubkey: Option<SenderPublicKey>) -> Self {
    Self {
      inner: FramedRead::new(inner, Default::default()),
      keys,
      sender_pubkey,
      session_keys: vec![],
    }
  }
}

impl<R> Stream for Decryptor<R>
where
  R: AsyncRead,
{
  type Item = Result<DataBlockDecryptor>;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let this = self.project();
    let item = this.inner.poll_next(cx);

    match ready!(item) {
      Some(Ok(buf)) => {
        match buf {
          DecodedBlock::HeaderInfo(_) => {
            // Nothing to do on header info.
            Poll::Pending
          }
          DecodedBlock::HeaderPacket(header_packet) => {
            // This block should be asynchronous in the number of header packets.
            let header_packet = Pin::new(&mut HeaderPacketDecryptor::new(
              header_packet,
              this.keys.clone(),
              this.sender_pubkey.clone(),
            ))
            .project()
            .handle
            .poll(cx);

            match ready!(header_packet) {
              Ok(Ok(mut header_packet)) => {
                // Todo consider edit packets.
                this
                  .session_keys
                  .append(&mut header_packet.data_enc_packets);

                // Return pending if the header packet has been processed
                Poll::Pending
              }
              Ok(Err(_)) => {
                // According to the spec, we should ignore invalid packets until
                // they have all been processed.
                Poll::Pending
              }
              Err(err) => {
                // Return an error only if there is some concurrency error.
                Poll::Ready(Some(Err(JoinHandleError(err))))
              }
            }
          }
          DecodedBlock::DataBlock(bytes) => {
            // This block shouldn't execute until all the header packets have been processed.
            // If we get here and there are no session keys, then return an error, otherwise
            // decode the data blocks.
            if this.session_keys.is_empty() {
              Poll::Ready(Some(Err(Crypt4GHError(NoSupportedEncryptionMethod))))
            } else {
              Poll::Ready(Some(Ok(DataBlockDecryptor::new(
                bytes,
                // Todo make this so it doesn't use owned Keys and SenderPublicKey as it will be called asynchronously.
                this.session_keys.clone(),
              ))))
            }
          }
        }
      }
      Some(Err(e)) => Poll::Ready(Some(Err(e))),
      None => Poll::Ready(None),
    }
  }
}

pin_project! {
    pub struct HeaderPacketDecryptor {
        #[pin]
        handle: JoinHandle<Result<DecryptedHeaderPackets>>
    }
}

impl HeaderPacketDecryptor {
  fn new(header_packet: Bytes, keys: Vec<Keys>, sender_pubkey: Option<SenderPublicKey>) -> Self {
    Self {
      handle: tokio::task::spawn_blocking(move || {
        HeaderPacketDecryptor::decrypt(header_packet, keys, sender_pubkey)
      }),
    }
  }

  fn decrypt(
    header_packet: Bytes,
    keys: Vec<Keys>,
    sender_pubkey: Option<SenderPublicKey>,
  ) -> Result<DecryptedHeaderPackets> {
    Ok(deconstruct_header_body(
      vec![header_packet.to_vec()],
      keys.as_slice(),
      &sender_pubkey.map(|pubkey| pubkey.bytes),
    )?)
  }
}

impl Future for HeaderPacketDecryptor {
  type Output = Result<DecryptedHeaderPackets>;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    self.project().handle.poll(cx).map_err(JoinHandleError)?
  }
}

#[derive(Debug)]
pub struct PlainTextBytes(Bytes);

pin_project! {
    pub struct DataBlockDecryptor {
        #[pin]
        handle: JoinHandle<Result<PlainTextBytes>>
    }
}

impl DataBlockDecryptor {
  fn new(data_block: Bytes, keys: Vec<Vec<u8>>) -> Self {
    Self {
      handle: tokio::task::spawn_blocking(move || DataBlockDecryptor::decrypt(data_block, keys)),
    }
  }

  fn decrypt(data_block: Bytes, keys: Vec<Vec<u8>>) -> Result<PlainTextBytes> {
    let read_buf = Cursor::new(data_block.to_vec());
    let mut write_buf = Cursor::new(vec![]);
    // Todo allow limit to be passed here.
    let mut write_info = WriteInfo::new(0, None, &mut write_buf);

    body_decrypt(read_buf, keys.as_slice(), &mut write_info, 0).map_err(Crypt4GHError)?;

    Ok(PlainTextBytes(write_buf.into_inner().into()))
  }
}

impl Future for DataBlockDecryptor {
  type Output = Result<PlainTextBytes>;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    self.project().handle.poll(cx).map_err(JoinHandleError)?
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::storage::crypt4gh::decrypt::decoder::tests::{
    assert_first_data_block, assert_first_header_packet, get_first_data_block,
    get_first_header_packet,
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

  #[tokio::test]
  async fn data_block_decryptor() {
    let (header_packets, data_block) = get_first_data_block().await;

    let data = DataBlockDecryptor::new(data_block, header_packets.data_enc_packets)
      .await
      .unwrap();

    assert_first_data_block(data.0.to_vec()).await;
  }
}
