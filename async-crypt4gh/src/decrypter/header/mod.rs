use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::AsyncRead;

use crate::decrypter::DecrypterStream;
use crate::decrypter::Result;

pub mod packets;

/// A struct which will poll a decrypter stream until the session keys are found.
/// After polling the future, the underlying decrypter stream should have processed
/// the session keys.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct SessionKeysFuture<'a, R> {
  handle: &'a mut DecrypterStream<R>,
}

impl<'a, R> SessionKeysFuture<'a, R> {
  /// Create the future.
  pub fn new(handle: &'a mut DecrypterStream<R>) -> Self {
    Self { handle }
  }

  /// Get the inner handle.
  pub fn get_mut(&mut self) -> &mut DecrypterStream<R> {
    self.handle
  }
}

impl<'a, R> Future for SessionKeysFuture<'a, R>
where
  R: AsyncRead + Unpin,
{
  type Output = Result<()>;

  fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    self.handle.poll_session_keys_unpin(cx)
  }
}
