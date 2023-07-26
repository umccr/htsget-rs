pub mod data_block;
pub mod header_packet;

use crate::storage::crypt4gh::decoder::Block;
use crate::storage::crypt4gh::decoder::DecodedBlock;
use crate::storage::crypt4gh::decrypter::data_block::DataBlockDecrypter;
use crate::storage::crypt4gh::decrypter::header_packet::HeaderPacketsDecrypter;
use crate::storage::crypt4gh::error::Error::{Crypt4GHError, JoinHandleError};
use crate::storage::crypt4gh::error::Result;
use crate::storage::crypt4gh::SenderPublicKey;
use bytes::Bytes;
use crypt4gh::error::Crypt4GHError::NoSupportedEncryptionMethod;
use crypt4gh::Keys;
use futures::ready;
use futures::Stream;
use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::io::Write;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncRead;
use tokio_util::codec::FramedRead;

pub struct DecrypterState {

}

pin_project! {
    /// A decrypter for an entire AsyncRead Crypt4GH file.
    pub struct DecrypterStream<R> {
        #[pin]
        inner: FramedRead<R, Block>,
        keys: Vec<Keys>,
        sender_pubkey: Option<SenderPublicKey>,
        session_keys: Vec<Vec<u8>>,
        header_packets: Vec<Bytes>
    }
}

impl<R> DecrypterStream<R>
where
  R: AsyncRead,
{
  /// Create a new decrypter.
  pub fn new(inner: R, keys: Vec<Keys>, sender_pubkey: Option<SenderPublicKey>) -> Self {
    Self {
      inner: FramedRead::new(inner, Default::default()),
      keys,
      sender_pubkey,
      session_keys: vec![],
      header_packets: vec![],
    }
  }

  /// Polls a header packet. This operation should be asynchronous in the number of header packets.
  pub fn poll_header_packet(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    header_packet: Bytes,
  ) -> Poll<Option<Result<DataBlockDecrypter>>> {
    todo!();
    // let this = self.project();
    //
    // let header_packet = Pin::new(&mut HeaderPacketDecryptor::new(
    //   header_packet,
    //   this.keys.clone(),
    //   this.sender_pubkey.clone(),
    // ))
    // .poll(cx);
    //
    // match header_packet {
    //   Poll::Ready(header_packet) => match header_packet {
    //     Ok(mut header_packet) => {
    //       // Todo consider edit packets.
    //       this
    //           .session_keys
    //           .append(&mut header_packet.data_enc_packets);
    //
    //       cx.waker().wake_by_ref();
    //       // Return pending if the header packet has been processed
    //       Poll::Pending
    //     }
    //     Err(err) => match err {
    //       // Only return an error only if there is some concurrency error.
    //       err @ JoinHandleError(_) => Poll::Ready(Some(Err(err))),
    //       _ => {
    //         cx.waker().wake_by_ref();
    //         Poll::Pending
    //       },
    //     },
    //   }
    //   Poll::Pending => {
    //     cx.waker().wake_by_ref();
    //     Poll::Pending
    //   }
    // }
  }

  /// Polls a data block. This function shouldn't execute until all the header packets have been
  /// processed. If there are no session keys, this function returns an error, otherwise it
  /// decrypts the data blocks.
  pub fn poll_data_block(
    self: Pin<&mut Self>,
    data_block: Bytes,
  ) -> Poll<Option<Result<DataBlockDecrypter>>> {
    let this = self.project();

    if this.session_keys.is_empty() {
      Poll::Ready(Some(Err(Crypt4GHError(NoSupportedEncryptionMethod.to_string()))))
    } else {
      Poll::Ready(Some(Ok(DataBlockDecrypter::new(
        data_block,
        // Todo make this so it doesn't use owned Keys and SenderPublicKey as it will be called asynchronously.
        this.session_keys.clone(),
      ))))
    }
  }
}

impl<R> Stream for DecrypterStream<R>
where
  R: AsyncRead,
{
  type Item = Result<DataBlockDecrypter>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let mut item = self.as_mut().project().inner.poll_next(cx);

    if let Poll::Ready(Some(Ok(DecodedBlock::HeaderInfo(_)))) = item {
      item = self.as_mut().project().inner.poll_next(cx);

      if let Poll::Ready(Some(Ok(DecodedBlock::HeaderInfo(_)))) = item {
        return Poll::Ready(Some(Err(Crypt4GHError("invalid Crypt4GH file".to_string()))))
      }
    }

    if let Poll::Ready(Some(Ok(DecodedBlock::HeaderPacket(header_packet)))) = item {
      self.header_packets.push(header_packet);

      return Poll::Pending;
    }


    match ready!(item) {
      Some(Ok(buf)) => {
        match buf {
          DecodedBlock::HeaderInfo(_) => {
            // Nothing to do on header info.
            // Need to ensure that the future is polled again.
            cx.waker().wake_by_ref();
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::storage::crypt4gh::tests::get_keys;
  use futures_util::future::join_all;
  use futures_util::StreamExt;
  use htsget_test::http_tests::get_test_file;
  use tokio::io::AsyncReadExt;

  #[tokio::test]
  async fn decrypter_stream() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = DecrypterStream::new(
      src,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    );

    let mut futures = vec![];
    while let Some(block) = stream.next().await {
      futures.push(block.unwrap());
    }
    //
    // // This should also test the concurrency of the data block futures as they are joined asynchronously.
    // let decrypted_bytes = join_all(futures).await.into_iter().fold(vec![], |mut acc, bytes| {
    //   acc.extend(bytes.unwrap().into_inner());
    //   acc
    // });
    //
    // let mut original_file = get_test_file("bam/htsnexus_test_NA12878.bam").await;
    // let mut original_bytes = vec![];
    // original_file.read_to_end(&mut original_bytes).await.unwrap();
    //
    // // Assert that the decrypted bytes are equal to the original file.
    // assert_eq!(decrypted_bytes, original_bytes);
  }
}
