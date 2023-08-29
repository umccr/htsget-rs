use std::future::Future;
use std::io;
use std::io::SeekFrom;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use crypt4gh::Keys;
use futures::ready;
use futures::Stream;
use pin_project_lite::pin_project;
use tokio::io::{AsyncRead, AsyncSeek};
use tokio_util::codec::FramedRead;

use crate::decoder::Block;
use crate::decoder::DecodedBlock;
use crate::decrypter::data_block::DataBlockDecrypter;
use crate::decrypter::header_packet::HeaderPacketsDecrypter;
use crate::decrypter::SeekState::{NotSeeking, SeekingToDataBlock, SeekingToPosition};
use crate::error::Error::Crypt4GHError;
use crate::error::Result;
use crate::SenderPublicKey;

pub mod data_block;
pub mod header_packet;

#[derive(Debug)]
enum SeekState {
  SeekingToPosition,
  SeekingToDataBlock,
  NotSeeking,
}

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
        seek_state: SeekState,
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
      seek_state: NotSeeking,
    }
  }

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

  /// Clamps the byte position to the nearest data block if the header length is known.
  pub fn clamp_position(&self, position: u64) -> Option<u64> {
    self.header_length().map(|length| {
      if position < length {
        length
      } else {
        let remainder = (position - length) % Block::standard_data_block_size();

        position - remainder
      }
    })
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

/// See the documentation for the trait for all functionality. This implementation ensures that all
/// seeks are aligned to the start of a data block preceding the requested seek position. Seek calls
/// also process the header packets in order to obtain the session keys.
///
///
/// No attempt is made to update the current_block_size so it will be set to whatever it was prior
/// to calling seek. Seeking past the end of the stream is allowed but the behaviour is dependent
/// on the underlying reader. Data block positions past the end of the stream may not be valid.
impl<R> AsyncSeek for DecrypterStream<R>
where
  R: AsyncRead + AsyncSeek + Unpin,
{
  fn start_seek(mut self: Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
    match self.seek_state {
      SeekingToPosition | SeekingToDataBlock => Err(io::Error::new(
        io::ErrorKind::Other,
        "cannot start_seek while another seek is in progress",
      )),
      NotSeeking => {
        self.seek_state = SeekingToPosition;

        self.project().inner.get_pin_mut().start_seek(position)
      }
    }
  }

  fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
    // When seeking the session keys are required.
    if let Err(err) = ready!(self.as_mut().poll_session_keys(cx)) {
      return Poll::Ready(Err(err.into()));
    }

    if let SeekingToPosition | SeekingToDataBlock = self.seek_state {
      // Finish any remaining seeking.
      let position = match ready!(self
        .as_mut()
        .project()
        .inner
        .get_pin_mut()
        .poll_complete(cx))
      {
        Ok(position) => position,
        Err(err) => {
          self.seek_state = NotSeeking;
          return Poll::Ready(Err(err));
        }
      };

      // If seeking to a position, we might still need to seek again to align with a data block.
      if let SeekingToPosition = self.seek_state {
        let data_block_position = self.clamp_position(position).ok_or_else(|| {
          io::Error::new(io::ErrorKind::Other, "could not find data block position")
        })?;

        if position != data_block_position {
          self.seek_state = SeekingToDataBlock;

          // Start seeking to the data block if required.
          self
            .project()
            .inner
            .get_pin_mut()
            .start_seek(SeekFrom::Start(data_block_position))?;

          cx.waker().wake_by_ref();
          return Poll::Pending;
        }
      }

      // Otherwise, this position must be a data block position.
      self.seek_state = NotSeeking;
      self.inner.read_buffer_mut().clear();

      Poll::Ready(Ok(position))
    } else {
      Poll::Ready(Ok(0))
    }
  }
}

#[cfg(test)]
mod tests {
  use bytes::BytesMut;
  use futures_util::future::join_all;
  use futures_util::StreamExt;
  use tokio::io::AsyncSeekExt;

  use htsget_test::http_tests::get_test_file;

  use crate::decoder::tests::assert_last_data_block;
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

  #[tokio::test]
  async fn clamp_position_first_data_block() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = DecrypterStream::new(
      src,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    );
    let _ = stream.next().await.unwrap().unwrap().await;

    assert_eq!(stream.clamp_position(0), Some(124));
    assert_eq!(stream.clamp_position(124), Some(124));
    assert_eq!(stream.clamp_position(200), Some(124));
  }

  #[tokio::test]
  async fn clamp_position_second_data_block() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = DecrypterStream::new(
      src,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    );
    let _ = stream.next().await.unwrap().unwrap().await;

    assert_eq!(stream.clamp_position(80000), Some(124 + 65564));
  }

  #[tokio::test]
  async fn seek_first_data_block() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut stream = DecrypterStream::new(
      src,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    );

    let seek = stream.seek(SeekFrom::Start(200)).await.unwrap();

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

    let mut stream = DecrypterStream::new(
      src,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    );

    let seek = stream.seek(SeekFrom::Start(80000)).await.unwrap();

    assert_eq!(seek, 124 + 65564);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);

    let seek = stream.seek(SeekFrom::Current(-20000)).await.unwrap();

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

    let mut stream = DecrypterStream::new(
      src,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    );

    let seek = stream.seek(SeekFrom::End(-1000)).await.unwrap();

    assert_eq!(seek, 2598043 - 40923);
    assert_eq!(stream.header_length(), Some(124));
    assert_eq!(stream.current_block_size(), None);

    let block = stream.next().await.unwrap().unwrap().await.unwrap();
    assert_last_data_block(block.bytes.to_vec()).await;
  }
}
