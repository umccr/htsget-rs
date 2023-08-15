use std::pin::Pin;
use std::task::{Context, Poll};
use std::{cmp, io};

use futures::ready;
use futures::stream::TryBuffered;
use futures::Stream;
use pin_project_lite::pin_project;
use tokio::io::{AsyncBufRead, AsyncRead, ReadBuf};

use super::decrypter::DecrypterStream;
use super::PlainTextBytes;

pub mod builder;

pin_project! {
    pub struct Reader<R>
      where R: AsyncRead
    {
      #[pin]
      stream: TryBuffered<DecrypterStream<R>>,
      worker_count: usize,
      bytes: PlainTextBytes,
      position: usize,
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

    // If the position is past the end of the buffer, then all the data has been read and a new
    // buffer should be initialised.
    if *this.position >= this.bytes.len() {
      match ready!(this.stream.poll_next(cx)) {
        Some(Ok(block)) => {
          // Once we have a new buffer, reinitialise the position and buffer.
          *this.bytes = block;
          *this.position = 0;
        }
        Some(Err(e)) => return Poll::Ready(Err(e.into())),
        None => return Poll::Ready(Ok(&[])),
      }
    }

    // Return the unconsumed data from the buffer.
    Poll::Ready(Ok(&this.bytes[*this.position..]))
  }

  fn consume(self: Pin<&mut Self>, amt: usize) {
    let this = self.project();
    // Update the position until the consumed amount reaches the end of the buffer.
    *this.position = cmp::min(*this.position + amt, this.bytes.len());
  }
}

#[cfg(test)]
mod tests {
  use futures_util::TryStreamExt;
  use noodles::bam::AsyncReader;
  use noodles::sam::Header;
  use tokio::io::AsyncReadExt;

  use htsget_test::http_tests::get_test_file;

  use crate::storage::crypt4gh::reader::builder::Builder;
  use crate::storage::crypt4gh::tests::{get_keys, get_original_file};
  use crate::storage::crypt4gh::SenderPublicKey;

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
      println!("{:?}", record);
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
