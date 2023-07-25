pub mod data_block;
pub mod header_packet;

use crate::storage::crypt4gh::decoder::Block;
use crate::storage::crypt4gh::decoder::DecodedBlock;
use crate::storage::crypt4gh::decryptor::data_block::DataBlockDecryptor;
use crate::storage::crypt4gh::decryptor::header_packet::HeaderPacketDecryptor;
use crate::storage::crypt4gh::error::Error::{Crypt4GHError, JoinHandleError};
use crate::storage::crypt4gh::error::{Error, Result};
use crate::storage::crypt4gh::SenderPublicKey;
use bytes::Bytes;
use crypt4gh::error::Crypt4GHError::NoSupportedEncryptionMethod;
use crypt4gh::Keys;
use futures::ready;
use futures::Stream;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncRead;
use tokio_util::codec::FramedRead;

pin_project! {
    /// A decryptor for an entire AsyncRead Crypt4GH file.
    pub struct Decryptor<R> {
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
  /// Create a new decryptor.
  pub fn new(inner: R, keys: Vec<Keys>, sender_pubkey: Option<SenderPublicKey>) -> Self {
    Self {
      inner: FramedRead::new(inner, Default::default()),
      keys,
      sender_pubkey,
      session_keys: vec![],
    }
  }

  /// Polls a header packet. This operation should be asynchronous in the number of header packets.
  pub fn poll_header_packet(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    header_packet: Bytes,
  ) -> Poll<Option<Result<DataBlockDecryptor>>> {
    let this = self.project();

    let header_packet = Pin::new(&mut HeaderPacketDecryptor::new(
      header_packet,
      this.keys.clone(),
      this.sender_pubkey.clone(),
    ))
    .poll(cx);

    match ready!(header_packet) {
      Ok(mut header_packet) => {
        // Todo consider edit packets.
        this
          .session_keys
          .append(&mut header_packet.data_enc_packets);

        // Return pending if the header packet has been processed
        Poll::Pending
      }
      Err(err) => match err {
        // Only return an error only if there is some concurrency error.
        err @ JoinHandleError(_) => Poll::Ready(Some(Err(err))),
        _ => Poll::Pending,
      },
    }
  }

  /// Polls a data block. This function shouldn't execute until all the header packets have been
  /// processed. If there are no session keys, this function returns an error, otherwise it
  /// decrypts the data blocks.
  pub fn poll_data_block(
    self: Pin<&mut Self>,
    data_block: Bytes,
  ) -> Poll<Option<Result<DataBlockDecryptor>>> {
    let this = self.project();

    if this.session_keys.is_empty() {
      Poll::Ready(Some(Err(Crypt4GHError(NoSupportedEncryptionMethod))))
    } else {
      Poll::Ready(Some(Ok(DataBlockDecryptor::new(
        data_block,
        // Todo make this so it doesn't use owned Keys and SenderPublicKey as it will be called asynchronously.
        this.session_keys.clone(),
      ))))
    }
  }
}

impl<R> Stream for Decryptor<R>
where
  R: AsyncRead,
{
  type Item = Result<DataBlockDecryptor>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let this = self.as_mut().project();
    let item = this.inner.poll_next(cx);

    match ready!(item) {
      Some(Ok(buf)) => {
        match buf {
          DecodedBlock::HeaderInfo(_) => {
            // Nothing to do on header info.
            Poll::Pending
          }
          DecodedBlock::HeaderPacket(header_packet) => self.poll_header_packet(cx, header_packet),
          DecodedBlock::DataBlock(data_block) => self.poll_data_block(data_block),
        }
      }
      Some(Err(e)) => Poll::Ready(Some(Err(e))),
      None => Poll::Ready(None),
    }
  }
}
