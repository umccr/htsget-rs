use std::io::SeekFrom;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{cmp, io};

use async_trait::async_trait;
use crypt4gh::header::HeaderInfo;
use crypt4gh::Keys;
use futures::ready;
use futures::stream::TryBuffered;
use futures::Stream;
use pin_project_lite::pin_project;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncSeek, ReadBuf};

use crate::advance::Advance;
use crate::decoder::Block;
use crate::error::Error::NumericConversionError;
use crate::error::Result;
use crate::reader::builder::Builder;
use crate::{DecryptedDataBlock, EncryptedHeaderPacketBytes};

use super::decrypter::DecrypterStream;

pub mod builder;

pin_project! {
    pub struct Reader<R>
      where R: AsyncRead
    {
      #[pin]
      stream: TryBuffered<DecrypterStream<R>>,
      current_block: DecryptedDataBlock,
      // The current position in the decrypted buffer.
      buf_position: usize,
      // The encrypted position of the current data block minus the size of the header.
      block_position: Option<u64>
    }
}

impl<R> Reader<R>
where
  R: AsyncRead,
{
  /// Gets the position of the data block which includes the current position of the underlying
  /// reader. This function will return a value that always corresponds the beginning of a data
  /// block or `None` if the reader has not read any bytes.
  pub fn current_block_position(&self) -> Option<u64> {
    self.block_position
  }

  /// Gets the position of the next data block from the current position of the underlying reader.
  /// This function will return a value that always corresponds the beginning of a data block, the
  /// size of the file, or `None` if the reader has not read any bytes.
  pub fn next_block_position(&self) -> Option<u64> {
    self.block_position.and_then(|block_position| {
      self
        .stream
        .get_ref()
        .clamp_position(block_position + Block::standard_data_block_size())
    })
  }

  /// Get a reference to the inner reader.
  pub fn get_ref(&self) -> &R {
    self.stream.get_ref().get_ref()
  }

  /// Get a mutable reference to the inner reader.
  pub fn get_mut(&mut self) -> &mut R {
    self.stream.get_mut().get_mut()
  }

  /// Get a pinned mutable reference to the inner reader.
  pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut R> {
    self.project().stream.get_pin_mut().get_pin_mut()
  }

  /// Get the inner reader.
  pub fn into_inner(self) -> R {
    self.stream.into_inner().into_inner()
  }

  /// Get the session keys. Empty before the header is polled.
  pub fn session_keys(&self) -> &[Vec<u8>] {
    self.stream.get_ref().session_keys()
  }

  /// Get the edit list packet. Empty before the header is polled.
  pub fn edit_list_packet(&self) -> Option<Vec<u64>> {
    self.stream.get_ref().edit_list_packet()
  }

  /// Get the header info.
  pub fn header_info(&self) -> Option<&HeaderInfo> {
    self.stream.get_ref().header_info()
  }

  /// Get the header size
  pub fn header_size(&self) -> Option<u64> {
    self.stream.get_ref().header_size()
  }

  /// Get the original encrypted header packets, not including the header info.
  pub fn encrypted_header_packets(&self) -> Option<&Vec<EncryptedHeaderPacketBytes>> {
    self.stream.get_ref().encrypted_header_packets()
  }

  /// Poll the reader until the header has been read.
  pub async fn read_header(&mut self) -> Result<()>
  where
    R: Unpin,
  {
    self.stream.get_mut().read_header().await
  }

  /// Get the reader's keys.
  pub fn keys(&self) -> &[Keys] {
    self.stream.get_ref().keys()
  }
}

impl<R> From<DecrypterStream<R>> for Reader<R>
where
  R: AsyncRead,
{
  fn from(stream: DecrypterStream<R>) -> Self {
    Builder::default().build_with_stream(stream)
  }
}

impl<R> AsyncRead for Reader<R>
where
  R: AsyncRead,
{
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<io::Result<()>> {
    // Defer to poll_fill_buf to do the read.
    let src = ready!(self.as_mut().poll_fill_buf(cx))?;

    // Calculate the correct amount to read and copy over to the read buf.
    let amt = cmp::min(src.len(), buf.remaining());
    buf.put_slice(&src[..amt]);

    // Inform the internal buffer that amt has been consumed.
    self.consume(amt);

    Poll::Ready(Ok(()))
  }
}

impl<R> AsyncBufRead for Reader<R>
where
  R: AsyncRead,
{
  fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
    let this = self.project();

    // If this is the beginning of the stream, set the block position to the header length, if any.
    if let (None, length @ Some(_)) = (
      this.block_position.as_ref(),
      this.stream.get_ref().header_size(),
    ) {
      *this.block_position = length;
    }

    // If the position is past the end of the buffer, then all the data has been read and a new
    // buffer should be initialised.
    if *this.buf_position >= this.current_block.len() {
      match ready!(this.stream.poll_next(cx)) {
        Some(Ok(block)) => {
          // Update the block position with the previous block size.
          *this.block_position = Some(
            this.block_position.unwrap_or_default()
              + u64::try_from(this.current_block.encrypted_size())
                .map_err(|_| NumericConversionError)?,
          );

          // We have a new buffer, reinitialise the position and buffer.
          *this.current_block = block;
          *this.buf_position = 0;
        }
        Some(Err(e)) => return Poll::Ready(Err(e.into())),
        None => return Poll::Ready(Ok(&[])),
      }
    }

    // Return the unconsumed data from the buffer.
    Poll::Ready(Ok(&this.current_block[*this.buf_position..]))
  }

  fn consume(self: Pin<&mut Self>, amt: usize) {
    let this = self.project();
    // Update the buffer position until the consumed amount reaches the end of the buffer.
    *this.buf_position = cmp::min(*this.buf_position + amt, this.current_block.len());
  }
}

impl<R> Reader<R>
where
  R: AsyncRead + AsyncSeek + Unpin + Send,
{
  /// Seek to a position in the encrypted stream.
  pub async fn seek_encrypted(&mut self, position: SeekFrom) -> io::Result<u64> {
    let position = self.stream.get_mut().seek_encrypted(position).await?;

    self.block_position = Some(position);

    Ok(position)
  }

  /// Seek to a position in the unencrypted stream.
  pub async fn seek_unencrypted(&mut self, position: u64) -> io::Result<u64> {
    let position = self.stream.get_mut().seek_unencrypted(position).await?;

    self.block_position = Some(position);

    Ok(position)
  }
}

#[async_trait]
impl<R> Advance for Reader<R>
where
  R: AsyncRead + Send + Unpin,
{
  async fn advance_encrypted(&mut self, position: u64) -> io::Result<u64> {
    let position = self.stream.get_mut().advance_encrypted(position).await?;

    self.block_position = Some(position);

    Ok(position)
  }

  async fn advance_unencrypted(&mut self, position: u64) -> io::Result<u64> {
    let position = self.stream.get_mut().advance_unencrypted(position).await?;

    self.block_position = Some(position);

    Ok(position)
  }

  fn stream_length(&self) -> Option<u64> {
    self.stream.get_ref().stream_length()
  }
}

#[cfg(test)]
mod tests {
  use std::io::SeekFrom;

  use futures_util::TryStreamExt;
  use noodles::bam::AsyncReader;
  use noodles::sam::Header;
  use tokio::io::AsyncReadExt;

  use htsget_test::http_tests::get_test_file;

  use crate::advance::Advance;
  use crate::reader::builder::Builder;
  use crate::tests::get_original_file;
  use crate::PublicKey;
  use htsget_test::crypt4gh::get_decryption_keys;

  #[tokio::test]
  async fn reader() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    let mut decrypted_bytes = vec![];
    reader.read_to_end(&mut decrypted_bytes).await.unwrap();

    let original_bytes = get_original_file().await;
    assert_eq!(decrypted_bytes, original_bytes);
  }

  #[tokio::test]
  async fn reader_with_noodles() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    let mut reader = AsyncReader::new(reader);

    let original_file = get_test_file("bam/htsnexus_test_NA12878.bam").await;
    let mut original_reader = AsyncReader::new(original_file);

    let header: Header = reader.read_header().await.unwrap().parse().unwrap();
    let reference_sequences = reader.read_reference_sequences().await.unwrap();

    let original_header: Header = original_reader
      .read_header()
      .await
      .unwrap()
      .parse()
      .unwrap();
    let original_reference_sequences = original_reader.read_reference_sequences().await.unwrap();

    assert_eq!(header, original_header);
    assert_eq!(reference_sequences, original_reference_sequences);

    let mut stream = original_reader.records(&original_header);
    let mut original_records = vec![];
    while let Some(record) = stream.try_next().await.unwrap() {
      original_records.push(record);
    }

    let mut stream = reader.records(&header);
    let mut records = vec![];
    while let Some(record) = stream.try_next().await.unwrap() {
      records.push(record);
    }

    assert_eq!(records, original_records);
  }

  #[tokio::test]
  async fn first_current_block_position() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the current block should not be known.
    assert_eq!(reader.current_block_position(), None);

    // Read the first byte of the decrypted data.
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf).await.unwrap();

    // Now the current position should be at the end of the header.
    assert_eq!(reader.current_block_position(), Some(124));
  }

  #[tokio::test]
  async fn first_next_block_position() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the next block should not be known.
    assert_eq!(reader.next_block_position(), None);

    // Read the first byte of the decrypted data.
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf).await.unwrap();

    // Now the next position should be at the second data block.
    assert_eq!(reader.next_block_position(), Some(124 + 65564));
  }

  #[tokio::test]
  async fn last_current_block_position() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the current block should not be known.
    assert_eq!(reader.current_block_position(), None);

    // Read the whole file.
    let mut decrypted_bytes = vec![];
    reader.read_to_end(&mut decrypted_bytes).await.unwrap();

    // Now the current position should be at the last data block.
    assert_eq!(reader.current_block_position(), Some(2598043 - 40923));
  }

  #[tokio::test]
  async fn last_next_block_position() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the next block should not be known.
    assert_eq!(reader.next_block_position(), None);

    // Read the whole file.
    let mut decrypted_bytes = vec![];
    reader.read_to_end(&mut decrypted_bytes).await.unwrap();

    // Now the next position should be the size of the file.
    assert_eq!(reader.next_block_position(), Some(2598043));
  }

  #[tokio::test]
  async fn seek_first_data_block() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.seek_encrypted(SeekFrom::Start(0)).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(124));
    assert_eq!(reader.next_block_position(), Some(124 + 65564));
  }

  #[tokio::test]
  async fn seek_to_end() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader
      .seek_encrypted(SeekFrom::Start(2598042))
      .await
      .unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(2598043 - 40923));
    assert_eq!(reader.next_block_position(), Some(2598043));
  }

  #[tokio::test]
  async fn seek_past_end() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader
      .seek_encrypted(SeekFrom::Start(2598044))
      .await
      .unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(2598043));
    assert_eq!(reader.next_block_position(), Some(2598043));
  }

  #[tokio::test]
  async fn seek_past_end_stream_length_override() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .with_stream_length(2598043)
      .build_with_reader(src, vec![recipient_private_key]);

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader
      .seek_encrypted(SeekFrom::Start(2598044))
      .await
      .unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(2598043));
    assert_eq!(reader.next_block_position(), Some(2598043));
  }

  #[tokio::test]
  async fn advance_first_data_block() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.advance_encrypted(0).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(124));
    assert_eq!(reader.next_block_position(), Some(124 + 65564));
  }

  #[tokio::test]
  async fn advance_to_end() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.advance_encrypted(2598042).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(2598043 - 40923));
    assert_eq!(reader.next_block_position(), Some(2598043));
  }

  #[tokio::test]
  async fn advance_past_end() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.advance_encrypted(2598044).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(2598043));
    assert_eq!(reader.next_block_position(), Some(2598043));
  }

  #[tokio::test]
  async fn advance_past_end_stream_length_override() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .with_stream_length(2598043)
      .build_with_reader(src, vec![recipient_private_key]);

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.advance_encrypted(2598044).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(2598043));
    assert_eq!(reader.next_block_position(), Some(2598043));
  }

  #[tokio::test]
  async fn seek_first_data_block_unencrypted() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.seek_unencrypted(0).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(124));
    assert_eq!(reader.next_block_position(), Some(124 + 65564));
  }

  #[tokio::test]
  async fn seek_to_end_unencrypted() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.seek_unencrypted(2596799).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(2598043 - 40923));
    assert_eq!(reader.next_block_position(), Some(2598043));
  }

  #[tokio::test]
  async fn seek_past_end_unencrypted() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.seek_unencrypted(2596800).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(2598043));
    assert_eq!(reader.next_block_position(), Some(2598043));
  }

  #[tokio::test]
  async fn seek_past_end_unencrypted_stream_length_override() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .with_stream_length(2598043)
      .build_with_reader(src, vec![recipient_private_key]);

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.seek_unencrypted(2596800).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(2598043));
    assert_eq!(reader.next_block_position(), Some(2598043));
  }

  #[tokio::test]
  async fn advance_first_data_block_unencrypted() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.advance_unencrypted(0).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(124));
    assert_eq!(reader.next_block_position(), Some(124 + 65564));
  }

  #[tokio::test]
  async fn advance_to_end_unencrypted() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.advance_unencrypted(2596799).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(2598043 - 40923));
    assert_eq!(reader.next_block_position(), Some(2598043));
  }

  #[tokio::test]
  async fn advance_past_end_unencrypted() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .build_with_stream_length(src, vec![recipient_private_key])
      .await
      .unwrap();

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.advance_unencrypted(2596800).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(2598043));
    assert_eq!(reader.next_block_position(), Some(2598043));
  }

  #[tokio::test]
  async fn advance_past_end_unencrypted_stream_length_override() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(sender_public_key))
      .with_stream_length(2598043)
      .build_with_reader(src, vec![recipient_private_key]);

    // Before anything is read the block positions should not be known.
    assert_eq!(reader.current_block_position(), None);
    assert_eq!(reader.next_block_position(), None);

    reader.advance_unencrypted(2596800).await.unwrap();

    // Now the positions should be at the first data block.
    assert_eq!(reader.current_block_position(), Some(2598043));
    assert_eq!(reader.next_block_position(), Some(2598043));
  }
}
