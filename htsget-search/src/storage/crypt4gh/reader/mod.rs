pub mod builder;

use super::decrypter::DecrypterStream;
use super::SenderPublicKey;
use crypt4gh::Keys;
use futures::stream::TryBuffered;
use futures_util::TryStreamExt;
use pin_project_lite::pin_project;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufRead, AsyncRead, ReadBuf};

pin_project! {
    pub struct Reader<R> where R: AsyncRead {
        #[pin]
        stream: Option<TryBuffered<DecrypterStream<R>>>,
        position: u64,
        worker_count: usize
    }
}

impl<R> AsyncRead for Reader<R>
where
  R: AsyncRead,
{
  fn poll_read(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<io::Result<()>> {
    todo!()
  }
}

impl<R> AsyncBufRead for Reader<R>
where
  R: AsyncRead,
{
  fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
    todo!()
  }

  fn consume(self: Pin<&mut Self>, _amt: usize) {
    todo!()
  }
}
