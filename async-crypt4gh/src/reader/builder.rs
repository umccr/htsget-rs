use std::thread;

use crypt4gh::Keys;
use futures_util::TryStreamExt;
use tokio::io::{AsyncRead, AsyncSeek};

use crate::decrypter::builder::Builder as DecrypterBuilder;
use crate::decrypter::DecrypterStream;
use crate::error::Result;
use crate::PublicKey;

use super::Reader;

/// An async Crypt4GH reader builder.
#[derive(Debug, Default)]
pub struct Builder {
  worker_count: Option<usize>,
  sender_pubkey: Option<PublicKey>,
  stream_length: Option<u64>,
}

impl Builder {
  /// Sets a worker count.
  pub fn with_worker_count(self, worker_count: usize) -> Self {
    self.set_worker_count(Some(worker_count))
  }

  /// Sets the sender public key
  pub fn with_sender_pubkey(self, sender_pubkey: PublicKey) -> Self {
    self.set_sender_pubkey(Some(sender_pubkey))
  }

  /// Sets a worker count.
  pub fn set_worker_count(mut self, worker_count: Option<usize>) -> Self {
    self.worker_count = worker_count;
    self
  }

  /// Sets the sender public key
  pub fn set_sender_pubkey(mut self, sender_pubkey: Option<PublicKey>) -> Self {
    self.sender_pubkey = sender_pubkey;
    self
  }

  /// Sets the stream length.
  pub fn with_stream_length(self, stream_length: u64) -> Self {
    self.set_stream_length(Some(stream_length))
  }

  /// Sets the stream length.
  pub fn set_stream_length(mut self, stream_length: Option<u64>) -> Self {
    self.stream_length = stream_length;
    self
  }

  /// Build the Crypt4GH reader.
  pub fn build_with_reader<R>(self, inner: R, keys: Vec<Keys>) -> Reader<R>
  where
    R: AsyncRead,
  {
    let worker_counter = self.worker_count();

    Reader {
      stream: DecrypterBuilder::default()
        .set_sender_pubkey(self.sender_pubkey)
        .set_stream_length(self.stream_length)
        .build(inner, keys)
        .try_buffered(worker_counter),
      // Dummy value for bytes to begin with.
      current_block: Default::default(),
      buf_position: 0,
      block_position: None,
    }
  }

  /// Build the reader and compute the stream length for seek operations.
  pub async fn build_with_stream_length<R>(self, inner: R, keys: Vec<Keys>) -> Result<Reader<R>>
  where
    R: AsyncRead + AsyncSeek + Unpin,
  {
    let stream_length = self.stream_length;
    let mut reader = self.build_with_reader(inner, keys);

    if stream_length.is_none() {
      reader.stream.get_mut().recompute_stream_length().await?;
    }

    Ok(reader)
  }

  /// Build the Crypt4GH reader with a decryper stream.
  pub fn build_with_stream<R>(self, stream: DecrypterStream<R>) -> Reader<R>
  where
    R: AsyncRead,
  {
    Reader {
      stream: stream.try_buffered(self.worker_count()),
      // Dummy value for bytes to begin with.
      current_block: Default::default(),
      buf_position: 0,
      block_position: None,
    }
  }

  fn worker_count(&self) -> usize {
    self.worker_count.unwrap_or_else(|| {
      thread::available_parallelism()
        .map(|worker_count| worker_count.get())
        .unwrap_or_else(|_| 1)
    })
  }
}
