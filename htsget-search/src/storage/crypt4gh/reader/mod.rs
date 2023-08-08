pub mod builder;

use super::decrypter::DecrypterStream;
use super::PlainTextBytes;
use super::SenderPublicKey;
use bytes::BufMut;
use crypt4gh::Keys;
use futures::ready;
use futures::stream::StreamExt;
use futures::stream::TryBuffered;
use futures::Stream;
use futures_util::TryStreamExt;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{cmp, io};
use tokio::io::{AsyncBufRead, AsyncRead, ReadBuf};

pin_project! {
    pub struct Reader<R> where R: AsyncRead {
        #[pin]
        stream:TryBuffered<DecrypterStream<R>>,
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

    // If the position is at the end of the buffer, then all the data has been read and a new
    // buffer should be initialised.
    if *this.position >= this.bytes.0.len() {
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
    Poll::Ready(Ok(&this.bytes.0[*this.position..]))
  }

  fn consume(self: Pin<&mut Self>, amt: usize) {
    let this = self.project();
    // Update the position until the consumed amount reaches the end of the buffer.
    *this.position += cmp::min(*this.position + amt, this.bytes.0.len());
  }
}
