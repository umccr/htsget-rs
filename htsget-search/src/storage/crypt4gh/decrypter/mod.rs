use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use crypt4gh::error::Crypt4GHError::NoSupportedEncryptionMethod;
use crypt4gh::Keys;
use futures::ready;
use futures::Stream;
use pin_project_lite::pin_project;
use tokio::io::AsyncRead;
use tokio_util::codec::FramedRead;

use crate::storage::crypt4gh::decoder::Block;
use crate::storage::crypt4gh::decoder::DecodedBlock;
use crate::storage::crypt4gh::decrypter::data_block::DataBlockDecrypter;
use crate::storage::crypt4gh::decrypter::header_packet::HeaderPacketsDecrypter;
use crate::storage::crypt4gh::error::Error::Crypt4GHError;
use crate::storage::crypt4gh::error::Result;
use crate::storage::crypt4gh::SenderPublicKey;

pub mod data_block;
pub mod header_packet;

pin_project! {
    /// A decrypter for an entire AsyncRead Crypt4GH file.
    pub struct DecrypterStream<R> {
        #[pin]
        inner: FramedRead<R, Block>,
        #[pin]
        header_packet_future: Option<HeaderPacketsDecrypter>,
        keys: Vec<Keys>,
        sender_pubkey: Option<SenderPublicKey>,
        session_keys: Vec<Vec<u8>>,
        edit_list_packet: Option<Vec<u64>>,
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
      header_packet_future: None,
      keys,
      sender_pubkey,
      session_keys: vec![],
      edit_list_packet: None,
    }
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
      Poll::Ready(Some(Err(Crypt4GHError(
        NoSupportedEncryptionMethod.to_string(),
      ))))
    } else {
      Poll::Ready(Some(Ok(DataBlockDecrypter::new(
        data_block,
        // Todo make this so it doesn't use owned Keys and SenderPublicKey as it will be called asynchronously.
        this.session_keys.clone(),
        this.edit_list_packet.clone(),
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
    if let Some(header_packet_decrypter) = self.as_mut().project().header_packet_future.as_pin_mut()
    {
      match header_packet_decrypter.poll(cx) {
        Poll::Ready(Ok(header_packets)) => {
          let mut this = self.as_mut().project();

          this.header_packet_future.set(None);
          this.session_keys.extend(header_packets.data_enc_packets);
          *this.edit_list_packet = header_packets.edit_list_packet;
        }
        Poll::Ready(Err(err)) => {
          return Poll::Ready(Some(Err(err)));
        }
        Poll::Pending => {
          return Poll::Pending;
        }
      }
    }

    let item = self.as_mut().project().inner.poll_next(cx);

    match ready!(item) {
      Some(Ok(buf)) => match buf {
        DecodedBlock::HeaderInfo(_) => {
          cx.waker().wake_by_ref();
          Poll::Pending
        }
        DecodedBlock::HeaderPackets(header_packets) => {
          let mut this = self.as_mut().project();
          this
            .header_packet_future
            .set(Some(HeaderPacketsDecrypter::new(
              header_packets,
              this.keys.clone(),
              this.sender_pubkey.clone(),
            )));

          cx.waker().wake_by_ref();
          Poll::Pending
        }
        DecodedBlock::DataBlock(data_block) => self.poll_data_block(data_block),
      },
      Some(Err(e)) => Poll::Ready(Some(Err(e))),
      None => Poll::Ready(None),
    }
  }
}

#[cfg(test)]
mod tests {
  use bytes::BytesMut;
  use futures_util::future::join_all;
  use futures_util::StreamExt;

  use htsget_test::http_tests::get_test_file;

  use crate::storage::crypt4gh::tests::{get_keys, get_original_file};

  use super::*;

  #[tokio::test]
  async fn decrypter_stream() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    // Todo allow limit to be passed here.
    let mut stream = DecrypterStream::new(
      src,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    );

    let mut futures = vec![];
    while let Some(block) = stream.next().await {
      futures.push(block.unwrap());
    }

    let decrypted_bytes =
      join_all(futures)
        .await
        .into_iter()
        .fold(BytesMut::new(), |mut acc, bytes| {
          acc.extend(bytes.unwrap().0);
          acc
        });

    // Assert that the decrypted bytes are equal to the original file bytes.
    let original_bytes = get_original_file().await;
    assert_eq!(decrypted_bytes, original_bytes);
  }
}
