use std::thread;

use bytes::Bytes;
use crypt4gh::Keys;
use futures_util::TryStreamExt;
use tokio::io::AsyncRead;

use crate::storage::crypt4gh::decrypter::DecrypterStream;
use crate::storage::crypt4gh::{PlainTextBytes, SenderPublicKey};

use super::Reader;

/// An async Crypt4GH reader builder.
#[derive(Debug, Default)]
pub struct Builder {
  worker_count: Option<usize>,
}

impl Builder {
  /// Sets a worker count.
  pub fn set_worker_count(mut self, worker_count: usize) -> Self {
    self.worker_count = Some(worker_count);
    self
  }

  /// Build the Crypt4GH reader.
  pub fn build<R>(
    self,
    reader: R,
    keys: Vec<Keys>,
    sender_pubkey: Option<SenderPublicKey>,
  ) -> Reader<R>
  where
    R: AsyncRead,
  {
    let worker_count = self.worker_count.unwrap_or_else(|| {
      thread::available_parallelism()
        .map(|worker_count| worker_count.get())
        .unwrap_or_else(|_| 1)
    });

    Reader {
      stream: DecrypterStream::new(reader, keys, sender_pubkey).try_buffered(worker_count),
      position: 0,
      worker_count,
      // Dummy value for bytes to begin with.
      bytes: PlainTextBytes(Bytes::new()),
    }
  }
}
