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

use crate::decoder::DecodedBlock;
use crate::decoder::{Block, HEADER_INFO_SIZE};
use crate::decrypter::data_block::DataBlockDecrypter;
use crate::decrypter::header_packet::HeaderPacketsDecrypter;
use crate::error::Error::Crypt4GHError;
use crate::error::Result;
use crate::SenderPublicKey;

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
        header_length: Option<usize>,
        current_block_size: Option<usize>,
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
      header_length: None,
      current_block_size: None,
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

  /// Get the length of the header, including the magic string, version number, packet count
  /// and the header packets. Returns `None` before the header packet is polled.
  pub fn header_length(&self) -> Option<usize> {
    self.header_length
  }

  /// Get the size of the current data block represented by the encrypted block returned by calling
  /// poll_next. This will equal `decoder::DATA_BLOCK_SIZE` except for the last block which may be
  /// less than that. Returns `None` before the first data block is polled.
  pub fn current_block_size(&self) -> Option<usize> {
    self.current_block_size
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
    let mut this = self.as_mut().project();

    match ready!(item) {
      Some(Ok(buf)) => match buf {
        DecodedBlock::HeaderInfo(_) => {
          cx.waker().wake_by_ref();
          Poll::Pending
        }
        DecodedBlock::HeaderPackets(header_packets) => {
          let (header_packets, header_length) = header_packets.into_inner();
          *this.header_length = Some(header_length + HEADER_INFO_SIZE);

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
        DecodedBlock::DataBlock(data_block) => {
          *this.current_block_size = Some(data_block.len());

          self.poll_data_block(data_block)
        }
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

  use crate::tests::{get_keys, get_original_file};

  use super::*;

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

    let decrypted_bytes =
      join_all(futures)
        .await
        .into_iter()
        .fold(BytesMut::new(), |mut acc, bytes| {
          let (bytes, _) = bytes.unwrap().into_inner();
          acc.extend(bytes.0);
          acc
        });

    // Assert that the decrypted bytes are equal to the original file bytes.
    let original_bytes = get_original_file().await;
    assert_eq!(decrypted_bytes, original_bytes);
  }

  #[tokio::test]
  async fn get_header_length() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = DecrypterStream::new(
      src,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    );

    assert!(stream.header_length().is_none());

    let _ = stream.next().await.unwrap().unwrap().await;

    assert_eq!(stream.header_length(), Some(124));
  }

  #[tokio::test]
  async fn first_block_size() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = DecrypterStream::new(
      src,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    );

    assert!(stream.current_block_size().is_none());

    let _ = stream.next().await.unwrap().unwrap().await;

    assert_eq!(stream.current_block_size(), Some(65564));
  }

  #[tokio::test]
  async fn last_block_size() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let stream = DecrypterStream::new(
      src,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    );

    assert!(stream.current_block_size().is_none());

    let mut stream = stream.skip(39);
    let _ = stream.next().await.unwrap().unwrap().await;

    assert_eq!(stream.get_ref().current_block_size(), Some(40923));
  }
}
