use std::pin::Pin;
use std::task::{Context, Poll};
use std::{cmp, io};

use crate::DecryptedDataBlock;
use futures::ready;
use futures::stream::TryBuffered;
use futures::Stream;
use pin_project_lite::pin_project;
use tokio::io::{AsyncBufRead, AsyncRead, ReadBuf};

use super::decrypter::DecrypterStream;

pub mod builder;

pin_project! {
    pub struct Reader<R>
      where R: AsyncRead
    {
      #[pin]
      stream: TryBuffered<DecrypterStream<R>>,
      worker_count: usize,
      current_block: DecryptedDataBlock,
      // The current position in the decrypted buffer.
      buf_position: usize,
      // The encrypted position of the current data block minus the size of the header.
      block_position: Option<usize>
    }
}

impl<R> Reader<R>
where
  R: AsyncRead,
{
  /// Gets the position of the data block which includes the current position of the underlying
  /// reader. This function will return a value that always corresponds the beginning of a data
  /// block or 0.
  pub fn current_block_position(&self) -> usize {
    self.block_position.unwrap_or_default()
  }

  /// Gets the position of the next data block from the current position of the underlying reader.
  /// This function will return a value that always corresponds the beginning of a data block or
  /// past the end of the file.
  pub fn next_block_position(&self) -> usize {
    self.block_position.unwrap_or_default() + self.current_block.encrypted_size()
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
    if this.block_position.unwrap_or_default() == 0 {
      *this.block_position = Some(
        this.block_position.unwrap_or_default()
          + this.stream.get_ref().header_length().unwrap_or_default(),
      );
    }

    // If the position is past the end of the buffer, then all the data has been read and a new
    // buffer should be initialised.
    if *this.buf_position >= this.current_block.len() {
      match ready!(this.stream.poll_next(cx)) {
        Some(Ok(block)) => {
          // Update the block position with the previous block size.
          *this.block_position =
            Some(this.block_position.unwrap_or_default() + this.current_block.encrypted_size());

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

#[cfg(test)]
mod tests {
  use futures_util::TryStreamExt;
  use noodles::bam::AsyncReader;
  use noodles::sam::Header;
  use tokio::io::AsyncReadExt;

  use htsget_test::http_tests::get_test_file;

  use crate::reader::builder::Builder;
  use crate::tests::{get_keys, get_original_file};
  use crate::SenderPublicKey;

  #[tokio::test]
  async fn reader() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut reader = Builder::default().build(
      src,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    );

    let mut decrypted_bytes = vec![];
    reader.read_to_end(&mut decrypted_bytes).await.unwrap();

    let original_bytes = get_original_file().await;
    assert_eq!(decrypted_bytes, original_bytes);
  }

  #[tokio::test]
  async fn reader_with_noodles() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let reader = Builder::default().build(
      src,
      vec![recipient_private_key],
      Some(SenderPublicKey::new(sender_public_key)),
    );
    let mut reader = AsyncReader::new(reader);

    let original_file = get_test_file("bam/htsnexus_test_NA12878.bam").await;
    let mut original_reader = AsyncReader::new(original_file);

    let header: Header = reader.read_header().await.unwrap().parse().unwrap();
    reader.read_reference_sequences().await.unwrap();

    let original_header: Header = original_reader
      .read_header()
      .await
      .unwrap()
      .parse()
      .unwrap();
    original_reader.read_reference_sequences().await.unwrap();

    assert_eq!(header, original_header);

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
}
