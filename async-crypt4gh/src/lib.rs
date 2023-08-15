use std::ops::Deref;

use bytes::Bytes;

pub use reader::builder::Builder as Crypt4GHReaderBuilder;
pub use reader::Reader as Crypt4GHReader;

pub mod decoder;
pub mod decrypter;
pub mod error;
pub mod reader;

/// A wrapper around a vec of bytes that represent a sender public key.
#[derive(Debug, Clone)]
pub struct SenderPublicKey {
  bytes: Vec<u8>,
}

impl SenderPublicKey {
  /// Create a new sender public key from bytes.
  pub fn new(bytes: Vec<u8>) -> Self {
    Self { bytes }
  }

  /// Get the inner bytes.
  pub fn into_inner(self) -> Vec<u8> {
    self.bytes
  }
}

/// A wrapper around a vec of bytes that represents plain text bytes.
#[derive(Debug, Clone)]
pub struct PlainTextBytes(Bytes);

impl PlainTextBytes {
  /// Create new plain text bytes from bytes.
  pub fn new(bytes: Bytes) -> Self {
    Self(bytes)
  }

  /// Get the inner bytes.
  pub fn into_inner(self) -> Bytes {
    self.0
  }

  /// Get the length of the inner bytes.
  pub const fn len(&self) -> usize {
    self.0.len()
  }

  /// Get the length of the inner bytes.
  pub const fn is_empty(&self) -> bool {
    self.0.is_empty()
  }
}

impl Deref for PlainTextBytes {
  type Target = [u8];

  #[inline]
  fn deref(&self) -> &[u8] {
    self.0.deref()
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use crypt4gh::keys::{get_private_key, get_public_key};
  use crypt4gh::Keys;
  use tokio::io::AsyncReadExt;

  use htsget_test::http_tests::{get_test_file, get_test_path};

  /// Returns the private keys of the recipient and the senders public key from the context of decryption.
  pub(crate) async fn get_keys() -> (Keys, Vec<u8>) {
    let recipient_private_key = get_private_key(&get_test_path("crypt4gh/keys/bob.sec"), || {
      Ok("".to_string())
    })
    .unwrap();
    let sender_public_key = get_public_key(&get_test_path("crypt4gh/keys/alice.pub")).unwrap();

    (
      Keys {
        method: 0,
        privkey: recipient_private_key,
        recipient_pubkey: sender_public_key.clone(),
      },
      sender_public_key,
    )
  }

  /// Get the original file bytes.
  pub(crate) async fn get_original_file() -> Vec<u8> {
    let mut original_file = get_test_file("bam/htsnexus_test_NA12878.bam").await;
    let mut original_bytes = vec![];

    original_file
      .read_to_end(&mut original_bytes)
      .await
      .unwrap();

    original_bytes
  }
}
