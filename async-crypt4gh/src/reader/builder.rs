use std::thread;

use crypt4gh::Keys;
use futures_util::TryStreamExt;
use tokio::io::AsyncRead;

use crate::decrypter::builder::Builder as DecrypterBuilder;
use crate::decrypter::DecrypterStream;
use crate::SenderPublicKey;

use super::Reader;

/// An async Crypt4GH reader builder.
#[derive(Debug, Default)]
pub struct Builder {
  worker_count: Option<usize>,
  length_hint: Option<u64>,
  sender_pubkey: Option<SenderPublicKey>,
}

impl Builder {
  /// Sets a worker count.
  pub fn with_worker_count(self, worker_count: usize) -> Self {
    self.set_worker_count(Some(worker_count))
  }

  /// Sets the length hint.
  pub fn with_length_hint(self, length_hint: u64) -> Self {
    self.set_length_hint(Some(length_hint))
  }

  /// Sets the sender public key
  pub fn with_sender_pubkey(self, sender_pubkey: SenderPublicKey) -> Self {
    self.set_sender_pubkey(Some(sender_pubkey))
  }

  /// Sets a worker count.
  pub fn set_worker_count(mut self, worker_count: Option<usize>) -> Self {
    self.worker_count = worker_count;
    self
  }

  /// Sets the length hint.
  pub fn set_length_hint(mut self, length_hint: Option<u64>) -> Self {
    self.length_hint = length_hint;
    self
  }

  /// Sets the sender public key
  pub fn set_sender_pubkey(mut self, sender_pubkey: Option<SenderPublicKey>) -> Self {
    self.sender_pubkey = sender_pubkey;
    self
  }

  /// Build the Crypt4GH reader.
  pub fn build_with_reader<R>(self, reader: R, keys: Vec<Keys>) -> Reader<R>
  where
    R: AsyncRead,
  {
    let worker_counter = self.worker_count();

    Reader {
      stream: DecrypterBuilder::default()
        .set_sender_pubkey(self.sender_pubkey)
        .set_length_hint(self.length_hint)
        .build(reader, keys)
        .try_buffered(worker_counter),
      // Dummy value for bytes to begin with.
      current_block: Default::default(),
      buf_position: 0,
      block_position: None,
      length_hint: self.length_hint,
    }
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
      length_hint: self.length_hint,
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
