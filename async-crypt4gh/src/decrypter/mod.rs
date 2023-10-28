use std::future::Future;
use std::io;
use std::io::SeekFrom;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use bytes::Bytes;
use crypt4gh::Keys;
use futures::ready;
use futures::Stream;
use pin_project_lite::pin_project;
use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt};
use tokio_util::codec::FramedRead;

use crate::advance::Advance;
use crate::decoder::Block;
use crate::decoder::DecodedBlock;
use crate::decrypter::data_block::DataBlockDecrypter;
use crate::decrypter::header::packets::HeaderPacketsDecrypter;
use crate::decrypter::header::SessionKeysFuture;
use crate::error::Error::Crypt4GHError;
use crate::error::Result;
use crate::SenderPublicKey;

pub mod builder;
pub mod data_block;
pub mod header;

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
        header_length: Option<u64>,
        current_block_size: Option<usize>,
        stream_length: Option<u64>,
    }
}

impl<R> DecrypterStream<R>
where
  R: AsyncRead,
{
  /// Polls a data block. This function shouldn't execute until all the header packets have been
  /// processed.
  pub fn poll_data_block(
    self: Pin<&mut Self>,
    data_block: Bytes,
  ) -> Poll<Option<Result<DataBlockDecrypter>>> {
    let this = self.project();

    Poll::Ready(Some(Ok(DataBlockDecrypter::new(
      data_block,
      // Todo make this so it doesn't use owned Keys and SenderPublicKey as it will be called asynchronously.
      this.session_keys.clone(),
      this.edit_list_packet.clone(),
    ))))
  }

  /// Poll the stream until the header packets and session keys are processed.
  pub fn poll_session_keys(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
    // Only execute this function if there are no session keys.
    if !self.session_keys.is_empty() {
      return Poll::Ready(Ok(()));
    }

    // Header packets are waiting to be decrypted.
    if let Some(header_packet_decrypter) = self.as_mut().project().header_packet_future.as_pin_mut()
    {
      return match ready!(header_packet_decrypter.poll(cx)) {
        Ok(header_packets) => {
          let mut this = self.as_mut().project();

          // Update the session keys and edit list packets.
          this.header_packet_future.set(None);
          this.session_keys.extend(header_packets.data_enc_packets);
          *this.edit_list_packet = header_packets.edit_list_packet;

          Poll::Ready(Ok(()))
        }
        Err(err) => Poll::Ready(Err(err)),
      };
    }

    // No header packets yet, so more data needs to be decoded.
    let mut this = self.as_mut().project();
    match ready!(this.inner.poll_next(cx)) {
      Some(Ok(buf)) => match buf {
        DecodedBlock::HeaderInfo(_) => {
          // Ignore the header info and poll again.
          cx.waker().wake_by_ref();
          Poll::Pending
        }
        DecodedBlock::HeaderPackets(header_packets) => {
          // Update the header length because we have access to the header packets.
          let (header_packets, header_length) = header_packets.into_inner();
          *this.header_length = Some(header_length + Block::header_info_size());

          // Add task for decrypting the header packets.
          this
            .header_packet_future
            .set(Some(HeaderPacketsDecrypter::new(
              header_packets,
              this.keys.clone(),
              this.sender_pubkey.clone(),
            )));

          // Poll again.
          cx.waker().wake_by_ref();
          Poll::Pending
        }
        DecodedBlock::DataBlock(_) => Poll::Ready(Err(Crypt4GHError(
          "data block reached without finding session keys".to_string(),
        ))),
      },
      Some(Err(e)) => Poll::Ready(Err(e)),
      None => Poll::Ready(Err(Crypt4GHError(
        "end of stream reached without finding session keys".to_string(),
      ))),
    }
  }

  /// Convenience for calling [`poll_session_keys`] on [`Unpin`] types.
  pub fn poll_session_keys_unpin(&mut self, cx: &mut Context<'_>) -> Poll<Result<()>>
  where
    Self: Unpin,
  {
    Pin::new(self).poll_session_keys(cx)
  }
}

impl<R> DecrypterStream<R> {
  /// An override for setting the stream length.
  pub async fn set_stream_length(&mut self, length: u64) {
    self.stream_length = Some(length);
  }

  /// Get a reference to the inner reader.
  pub fn get_ref(&self) -> &R {
    self.inner.get_ref()
  }

  /// Get a mutable reference to the inner reader.
  pub fn get_mut(&mut self) -> &mut R {
    self.inner.get_mut()
  }

  /// Get a pinned mutable reference to the inner reader.
  pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut R> {
    self.project().inner.get_pin_mut()
  }

  /// Get the inner reader.
  pub fn into_inner(self) -> R {
    self.inner.into_inner()
  }

  /// Get the length of the header, including the magic string, version number, packet count
  /// and the header packets. Returns `None` before the header packet is polled.
  pub fn header_length(&self) -> Option<u64> {
    self.header_length
  }

  /// Get the size of the current data block represented by the encrypted block returned by calling
  /// poll_next. This will equal `decoder::DATA_BLOCK_SIZE` except for the last block which may be
  /// less than that. Returns `None` before the first data block is polled.
  pub fn current_block_size(&self) -> Option<usize> {
    self.current_block_size
  }

  /// Clamps the byte position to the nearest data block if the header length is known. This
  /// function takes into account the stream length if it is present.
  pub fn clamp_position(&self, position: u64) -> Option<u64> {
    self.header_length().map(|length| {
      if position < length {
        length
      } else {
        match self.stream_length {
          Some(end_length) if position > end_length => end_length,
          _ => {
            let remainder = (position - length) % Block::standard_data_block_size();

            position - remainder
          }
        }
      }
    })
  }
}

impl<R> DecrypterStream<R>
where
  R: AsyncRead + AsyncSeek + Unpin,
{
  /// Recompute the stream length. Having a stream length means that data block positions past the
  /// end of the stream will be valid and will equal the the length of the stream. By default this
  /// struct contains no stream length when it is initialized.
  ///
  /// This can take up to 3 seek calls. If the size of the underlying buffer changes, this function
  /// should be called again, otherwise data block positions may not be valid.
  pub async fn recompute_stream_length(&mut self) -> Result<u64> {
    let inner = self.inner.get_mut();

    let position = inner.seek(SeekFrom::Current(0)).await?;
    let length = inner.seek(SeekFrom::End(0)).await?;

    if position != length {
      inner.seek(SeekFrom::Start(position)).await?;
    }

    self.stream_length = Some(length);

    Ok(length)
  }
}

impl<R> Stream for DecrypterStream<R>
where
  R: AsyncRead,
{
  type Item = Result<DataBlockDecrypter>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    // When polling, we first need to process enough data to get the session keys.
    if let Err(err) = ready!(self.as_mut().poll_session_keys(cx)) {
      return Poll::Ready(Some(Err(err)));
    }

    let this = self.as_mut().project();
    let item = this.inner.poll_next(cx);

    match ready!(item) {
      Some(Ok(buf)) => match buf {
        DecodedBlock::HeaderInfo(_) | DecodedBlock::HeaderPackets(_) => {
          // Session keys have already been read, so ignore the header info and header packets
          // and poll again
          cx.waker().wake_by_ref();
          Poll::Pending
        }
        DecodedBlock::DataBlock(data_block) => {
          // The new size of the data block is available, so update it.
          *this.current_block_size = Some(data_block.len());

          // Session keys have been obtained so process the data blocks.
          self.poll_data_block(data_block)
        }
      },
      Some(Err(e)) => Poll::Ready(Some(Err(e))),
      None => Poll::Ready(None),
    }
  }
}

impl<R> DecrypterStream<R>
where
  R: AsyncRead + AsyncSeek + Unpin + Send,
{
  pub async fn seek_encrypted(&mut self, position: SeekFrom) -> io::Result<u64> {
    // Make sure that session keys are polled.
    SessionKeysFuture::new(self).await?;

    // First poll to the position specified.
    let seek = self.inner.get_mut().seek(position).await?;

    // Then advance to the correct data block position.
    let advance = self.advance(seek).await?;

    // Then seek to the correct position.
    let seek = self.inner.get_mut().seek(SeekFrom::Start(advance)).await?;
    self.inner.read_buffer_mut().clear();

    Ok(seek)
  }
}

#[async_trait]
impl<R> Advance for DecrypterStream<R>
where
  R: AsyncRead + Send + Unpin,
{
  async fn advance(&mut self, position: u64) -> io::Result<u64> {
    // Make sure that session keys are polled.
    SessionKeysFuture::new(self).await?;

    // Get the next position.
    let data_block_position = self
      .clamp_position(position)
      .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "could not find data block position"))?;

    Ok(data_block_position)
  }

  fn stream_length(&self) -> Option<u64> {
    self.stream_length
  }
}

#[cfg(test)]
mod tests {
  use bytes::BytesMut;
  use futures_util::future::join_all;
  use futures_util::StreamExt;

  use htsget_test::http_tests::get_test_file;

  use crate::decoder::tests::assert_last_data_block;
  use crate::decrypter::builder::Builder;
  use crate::tests::{get_keys, get_original_file};

  use super::*;

  #[tokio::test]
  async fn decrypter_stream() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build(src, vec![recipient_private_key]);

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

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build(src, vec![recipient_private_key]);

    assert!(stream.header_length().is_none());

    let _ = stream.next().await.unwrap().unwrap().await;

    assert_eq!(stream.header_length(), Some(124));
  }

  #[tokio::test]
  async fn first_block_size() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build(src, vec![recipient_private_key]);

    assert!(stream.current_block_size().is_none());

    let _ = stream.next().await.unwrap().unwrap().await;

    assert_eq!(stream.current_block_size(), Some(65564));
  }

  #[tokio::test]
  async fn last_block_size() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build(src, vec![recipient_private_key]);

    assert!(stream.current_block_size().is_none());

    let mut stream = stream.skip(39);
    let _ = stream.next().await.unwrap().unwrap().await;

    assert_eq!(stream.get_ref().current_block_size(), Some(40923));
  }

  #[tokio::test]
  async fn clamp_position_first_data_block() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build(src, vec![recipient_private_key]);
    let _ = stream.next().await.unwrap().unwrap().await;

    assert_eq!(stream.clamp_position(0), Some(124));
    assert_eq!(stream.clamp_position(124), Some(124));
    assert_eq!(stream.clamp_position(200), Some(124));
  }

  #[tokio::test]
  async fn clamp_position_second_data_block() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build(src, vec![recipient_private_key]);
    let _ = stream.next().await.unwrap().unwrap().await;

    assert_eq!(stream.clamp_position(80000), Some(124 + 65564));
  }

  #[tokio::test]
  async fn seek_first_data_block() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build(src, vec![recipient_private_key]);

    let seek = stream.seek_encrypted(SeekFrom::Start(200)).await.unwrap();

    assert_eq!(seek, 124);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);

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
  async fn seek_second_data_block() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build(src, vec![recipient_private_key]);

    let seek = stream.seek_encrypted(SeekFrom::Start(80000)).await.unwrap();

    assert_eq!(seek, 124 + 65564);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);

    let seek = stream
      .seek_encrypted(SeekFrom::Current(-20000))
      .await
      .unwrap();

    assert_eq!(seek, 124);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);

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
  async fn seek_to_end() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build(src, vec![recipient_private_key]);

    let seek = stream.seek_encrypted(SeekFrom::End(-1000)).await.unwrap();

    assert_eq!(seek, 2598043 - 40923);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);

    let block = stream.next().await.unwrap().unwrap().await.unwrap();
    assert_last_data_block(block.bytes.to_vec()).await;
  }

  #[tokio::test]
  async fn seek_past_end() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    let seek = stream.seek_encrypted(SeekFrom::End(80000)).await.unwrap();

    assert_eq!(seek, 2598043);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);
    assert!(stream.next().await.is_none());
  }

  #[tokio::test]
  async fn seek_past_end_stream_length_override() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .with_stream_length(2598043)
      .build(src, vec![recipient_private_key]);

    let seek = stream.seek_encrypted(SeekFrom::End(80000)).await.unwrap();

    assert_eq!(seek, 2598043);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);
    assert!(stream.next().await.is_none());
  }

  #[tokio::test]
  async fn advance_first_data_block() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build(src, vec![recipient_private_key]);

    let advance = stream.advance(200).await.unwrap();

    assert_eq!(advance, 124);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);

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
  async fn advance_second_data_block() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build(src, vec![recipient_private_key]);

    let advance = stream.advance(80000).await.unwrap();

    assert_eq!(advance, 124 + 65564);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);

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
  async fn advance_to_end() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build(src, vec![recipient_private_key]);

    let advance = stream.advance(2598042).await.unwrap();

    assert_eq!(advance, 2598043 - 40923);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);

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
  async fn advance_past_end() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    let advance = stream.advance(2598044).await.unwrap();

    assert_eq!(advance, 2598043);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);

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
  async fn advance_past_end_stream_length_override() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .with_stream_length(2598043)
      .build(src, vec![recipient_private_key]);

    let advance = stream.advance(2598044).await.unwrap();

    assert_eq!(advance, 2598043);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);

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
}
