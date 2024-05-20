use base64::engine::general_purpose;
use base64::Engine;
use bstr::ByteSlice;
use crypt4gh::error::Crypt4GHError;
use crypt4gh::keys::generate_private_key;
use rustls::PrivateKey;
use std::cmp::min;
use std::io;
use std::ops::Add;

use crate::decoder::Block;
use crate::error::{Error, Result};
use crate::{KeyPair, PublicKey};

fn to_current_data_block(pos: u64, header_len: u64) -> u64 {
  header_len + (pos / Block::encrypted_block_size()) * Block::standard_data_block_size()
}

/// Convert an unencrypted file position to an encrypted position if the header length is known.
pub fn to_encrypted(position: u64, header_length: u64) -> u64 {
  let number_data_blocks = position / Block::encrypted_block_size();
  // Additional bytes include the full data block size.
  let mut additional_bytes = number_data_blocks * (Block::nonce_size() + Block::mac_size());

  // If there is left over data, then there are more nonce bytes.
  let remainder = position % Block::encrypted_block_size();
  if remainder != 0 {
    additional_bytes += Block::nonce_size();
  }

  // Then add the extra bytes to the current position.
  header_length + position + additional_bytes
}

/// Convert an encrypted file position to an unencrypted position if the header length is known.
pub fn to_unencrypted(encrypted_position: u64, header_length: u64) -> u64 {
  let number_data_blocks = encrypted_position / Block::standard_data_block_size();
  let mut additional_bytes = number_data_blocks * (Block::nonce_size() + Block::mac_size());

  let remainder = encrypted_position % Block::standard_data_block_size();
  if remainder != 0 {
    additional_bytes += Block::nonce_size();
  }

  encrypted_position - header_length - additional_bytes
}

/// Convert an unencrypted file size to an encrypted file size if the header length is known.
pub fn to_encrypted_file_size(file_size: u64, header_length: u64) -> u64 {
  to_encrypted(file_size, header_length) + Block::mac_size()
}

/// Convert an encrypted file size to an unencrypted file size if the header length is known.
pub fn to_unencrypted_file_size(encrypted_file_size: u64, header_length: u64) -> u64 {
  to_unencrypted(encrypted_file_size, header_length) - Block::mac_size()
}

/// Convert an unencrypted position to an encrypted position as shown in
/// https://samtools.github.io/hts-specs/crypt4gh.pdf chapter 4.1.
pub fn unencrypted_to_data_block(pos: u64, header_len: u64, encrypted_file_size: u64) -> u64 {
  min(encrypted_file_size, to_current_data_block(pos, header_len))
}

/// Get the next data block position from the unencrypted position.
pub fn unencrypted_to_next_data_block(pos: u64, header_len: u64, encrypted_file_size: u64) -> u64 {
  min(
    encrypted_file_size,
    to_current_data_block(pos, header_len) + Block::standard_data_block_size(),
  )
}

fn unencrypted_clamped_position(pos: u64, encrypted_file_size: u64) -> u64 {
  let data_block_positions = unencrypted_to_data_block(pos, 0, encrypted_file_size);
  let data_block_count = data_block_positions / Block::standard_data_block_size();

  data_block_positions - ((Block::nonce_size() + Block::mac_size()) * data_block_count)
}

/// Convert an unencrypted position to the additional bytes prior to the position that must be
/// included when encrypting data blocks.
pub fn unencrypted_clamp(pos: u64, encrypted_file_size: u64) -> u64 {
  min(
    to_unencrypted_file_size(encrypted_file_size, 0),
    unencrypted_clamped_position(pos, encrypted_file_size),
  )
}

/// Convert an unencrypted position to the additional bytes after to the position that must be
/// included when encrypting data blocks.
pub fn unencrypted_clamp_next(pos: u64, encrypted_file_size: u64) -> u64 {
  min(
    to_unencrypted_file_size(encrypted_file_size, 0),
    unencrypted_clamped_position(pos, encrypted_file_size) + Block::encrypted_block_size(),
  )
}

/// Generate a private and public key pair.
pub fn generate_key_pair() -> Result<KeyPair> {
  let skpk = generate_private_key()?;
  let (private_key, public_key) = skpk.split_at(32);

  Ok(KeyPair::new(
    PrivateKey(Vec::from(private_key)),
    PublicKey::new(Vec::from(public_key)),
  ))
}

pub async fn encode_public_key(public_key: PublicKey) -> String {
  let pk = String::new();
  let pk = pk.add("-----BEGIN CRYPT4GH PUBLIC KEY-----\n");

  let pk = pk.add(&general_purpose::STANDARD.encode(public_key.into_inner()));

  pk.add("\n-----END CRYPT4GH PUBLIC KEY-----\n")
}

/// Read a public key from bytes
pub async fn read_public_key(bytes: Vec<u8>) -> Result<PublicKey> {
  let mut lines = ByteSlice::lines(bytes.as_slice()).collect::<Vec<&[u8]>>();

  let error = || {
    Error::IOError(io::Error::new(
      io::ErrorKind::Other,
      "invalid public key".to_string(),
    ))
  };

  if lines.is_empty() {
    return Err(error());
  }

  // Optionally decode the key from a string.
  let key = if lines
    .first()
    .is_some_and(|first| first.contains_str(b"CRYPT4GH"))
    && lines
      .last()
      .is_some_and(|first| first.contains_str(b"CRYPT4GH"))
  {
    lines.remove(0);
    lines.pop();

    general_purpose::STANDARD
      .decode(lines.into_iter().flatten().copied().collect::<Vec<u8>>())
      .map_err(|e| Crypt4GHError::BadBase64Error(e.into()))?
  } else {
    lines.into_iter().flatten().copied().collect()
  };

  Ok(PublicKey::new(key))
}

#[cfg(test)]
mod tests {
  use crypt4gh::keys::get_public_key;
  use htsget_test::http::get_test_path;
  use std::fs;

  use super::*;
  use crate::util::{unencrypted_clamp, unencrypted_to_data_block, unencrypted_to_next_data_block};

  #[test]
  fn test_to_encrypted() {
    let pos = 80000;
    let expected = 120 + 65536 + 12 + 16;
    let result = unencrypted_to_data_block(pos, 120, to_encrypted_file_size(100000, 120));
    assert_eq!(result, expected);
  }

  #[test]
  fn test_to_encrypted_file_size() {
    let pos = 110000;
    let expected = 60148;
    let result = unencrypted_to_data_block(pos, 120, to_encrypted_file_size(60000, 120));
    assert_eq!(result, expected);
  }

  #[test]
  fn test_to_encrypted_pos_greater_than_file_size() {
    let pos = 110000;
    let expected = 120 + 65536 + 12 + 16;
    let result = unencrypted_to_data_block(pos, 120, to_encrypted_file_size(100000, 120));
    assert_eq!(result, expected);
  }

  #[test]
  fn test_next_data_block() {
    let pos = 100000;
    let expected = 120 + (65536 + 12 + 16) * 2;
    let result = unencrypted_to_next_data_block(pos, 120, to_encrypted_file_size(150000, 120));
    assert_eq!(result, expected);
  }

  #[test]
  fn test_next_data_block_file_size() {
    let pos = 110000;
    let expected = 100176;
    let result = unencrypted_to_next_data_block(pos, 120, to_encrypted_file_size(100000, 120));
    assert_eq!(result, expected);
  }

  #[test]
  fn test_unencrypted_clamp() {
    let pos = 0;
    let expected = 0;
    let result = unencrypted_clamp(pos, to_encrypted_file_size(5485112, 0));
    assert_eq!(result, expected);

    let pos = 145110;
    let expected = 131072;
    let result = unencrypted_clamp(pos, to_encrypted_file_size(5485112, 0));
    assert_eq!(result, expected);

    let pos = 5485074;
    let expected = 5439488;
    let result = unencrypted_clamp(pos, to_encrypted_file_size(5485112, 0));
    assert_eq!(result, expected);
  }

  #[test]
  fn test_unencrypted_clamp_next() {
    let pos = 7853;
    let expected = 65536;
    let result = unencrypted_clamp_next(pos, to_encrypted_file_size(5485112, 0));
    assert_eq!(result, expected);

    let pos = 453039;
    let expected = 458752;
    let result = unencrypted_clamp_next(pos, to_encrypted_file_size(5485112, 0));
    assert_eq!(result, expected);

    let pos = 5485112;
    let expected = 5485112;
    let result = unencrypted_clamp_next(pos, to_encrypted_file_size(5485112, 0));
    assert_eq!(result, expected);
  }

  #[tokio::test]
  async fn test_read_public_key_raw() {
    let test_key = vec![
      56, 44, 122, 180, 24, 116, 207, 149, 165, 49, 204, 77, 224, 136, 232, 121, 209, 249, 23, 51,
      120, 2, 187, 147, 82, 227, 232, 32, 17, 223, 7, 38,
    ];

    let result = read_public_key(test_key.clone()).await.unwrap();
    assert_eq!(result, PublicKey::new(test_key))
  }

  #[tokio::test]
  async fn test_read_public_key_with_header() {
    let expected_public_key = get_public_key(get_test_path("crypt4gh/keys/bob.pub")).unwrap();
    let result = read_public_key(fs::read(get_test_path("crypt4gh/keys/bob.pub")).unwrap())
      .await
      .unwrap();

    assert_eq!(result, PublicKey::new(expected_public_key))
  }
}
