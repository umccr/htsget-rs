use std::ops::Deref;

use bytes::Bytes;

pub use reader::builder::Builder as Crypt4GHReaderBuilder;
pub use reader::Reader as Crypt4GHReader;

pub mod advance;
pub mod decoder;
pub mod decrypter;
pub mod edit_lists;
pub mod error;
pub mod reader;
pub mod util;

/// A wrapper around a vec of bytes that represent a public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
  bytes: Vec<u8>,
}

impl PublicKey {
  /// Create a new sender public key from bytes.
  pub fn new(bytes: Vec<u8>) -> Self {
    Self { bytes }
  }

  /// Get the inner bytes.
  pub fn into_inner(self) -> Vec<u8> {
    self.bytes
  }
}

/// Represents an encrypted header packet with the packet length, and the remaining header.
#[derive(Clone, Debug, Default)]
pub struct EncryptedHeaderPacketBytes {
  packet_length: Bytes,
  header: Bytes,
}

impl EncryptedHeaderPacketBytes {
  /// Create header packet bytes.
  pub fn new(packet_length: Bytes, header: Bytes) -> Self {
    Self {
      packet_length,
      header,
    }
  }

  /// Get packet length bytes.
  pub fn packet_length(&self) -> &Bytes {
    &self.packet_length
  }

  /// Get header bytes.
  pub fn header(&self) -> &Bytes {
    &self.header
  }

  /// Get the owned packet length and header bytes.
  pub fn into_inner(self) -> (Bytes, Bytes) {
    (self.packet_length, self.header)
  }

  /// Get the header bytes only.
  pub fn into_header_bytes(self) -> Bytes {
    self.header
  }
}

/// Represents the encrypted header packet data, and the total size of all the header packets.
#[derive(Debug, Default)]
pub struct EncryptedHeaderPackets {
  header_packets: Vec<EncryptedHeaderPacketBytes>,
  header_length: u64,
}

impl EncryptedHeaderPackets {
  /// Create a new decrypted data block.
  pub fn new(header_packets: Vec<EncryptedHeaderPacketBytes>, size: u64) -> Self {
    Self {
      header_packets,
      header_length: size,
    }
  }

  /// Get the header packet bytes
  pub fn header_packets(&self) -> &Vec<EncryptedHeaderPacketBytes> {
    &self.header_packets
  }

  /// Get the size of all the packets.
  pub fn header_length(&self) -> u64 {
    self.header_length
  }

  /// Get the inner bytes and size.
  pub fn into_inner(self) -> (Vec<EncryptedHeaderPacketBytes>, u64) {
    (self.header_packets, self.header_length)
  }
}

/// Represents the decrypted data block and its original encrypted size.
#[derive(Debug, Default)]
pub struct DecryptedDataBlock {
  bytes: DecryptedBytes,
  encrypted_size: usize,
}

impl DecryptedDataBlock {
  /// Create a new decrypted data block.
  pub fn new(bytes: DecryptedBytes, encrypted_size: usize) -> Self {
    Self {
      bytes,
      encrypted_size,
    }
  }

  /// Get the plain text bytes.
  pub fn bytes(&self) -> &DecryptedBytes {
    &self.bytes
  }

  /// Get the encrypted size.
  pub fn encrypted_size(&self) -> usize {
    self.encrypted_size
  }

  /// Get the inner bytes and size.
  pub fn into_inner(self) -> (DecryptedBytes, usize) {
    (self.bytes, self.encrypted_size)
  }

  /// Get the length of the decrypted bytes.
  pub const fn len(&self) -> usize {
    self.bytes.len()
  }

  /// Check if the decrypted bytes are empty
  pub const fn is_empty(&self) -> bool {
    self.bytes.is_empty()
  }
}

impl Deref for DecryptedDataBlock {
  type Target = [u8];

  #[inline]
  fn deref(&self) -> &[u8] {
    self.bytes.deref()
  }
}

/// A wrapper around a vec of bytes that represents decrypted data.
#[derive(Debug, Default, Clone)]
pub struct DecryptedBytes(Bytes);

impl DecryptedBytes {
  /// Create new decrypted bytes from bytes.
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

  /// Check if the inner bytes are empty.
  pub const fn is_empty(&self) -> bool {
    self.0.is_empty()
  }
}

impl Deref for DecryptedBytes {
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
    let recipient_private_key =
      get_private_key(get_test_path("crypt4gh/keys/bob.sec"), Ok("".to_string())).unwrap();
    let sender_public_key = get_public_key(get_test_path("crypt4gh/keys/alice.pub")).unwrap();

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
