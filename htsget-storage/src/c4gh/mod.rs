//! This module contains `Storage` implementations for accessing Crypt4GH encrypted data.
//! These serve as wrappers around other `Storage` implementations.
//!

use crypt4gh::error::Crypt4GHError;
use crypt4gh::header::{DecryptedHeaderPackets, HeaderInfo};
use crypt4gh::{body_decrypt, body_decrypt_parts, header, Keys, WriteInfo};
use std::cmp::min;
use std::io;
use std::io::{BufWriter, Cursor, Read};

mod edit;
pub mod storage;

pub const ENCRYPTED_BLOCK_SIZE: u64 = 65536;
pub const NONCE_SIZE: u64 = 12; // ChaCha20 IETF Nonce size
pub const MAC_SIZE: u64 = 16;

const DATA_BLOCK_SIZE: u64 = NONCE_SIZE + ENCRYPTED_BLOCK_SIZE + MAC_SIZE;

/// Represents a C4GH which is deserialized into relevant information relevant to `C4GHStorage`.
#[derive(Debug)]
pub struct DeserializedHeader {
  pub(crate) header_info: HeaderInfo,
  pub(crate) session_keys: Vec<Vec<u8>>,
  pub(crate) header_size: u64,
  pub(crate) edit_list: Option<Vec<u64>>,
}

impl Clone for DeserializedHeader {
  fn clone(&self) -> Self {
    Self {
      header_info: HeaderInfo {
        magic_number: self.header_info.magic_number,
        version: self.header_info.version,
        packets_count: self.header_info.packets_count,
      },
      session_keys: self.session_keys.clone(),
      header_size: self.header_size,
      edit_list: self.edit_list.clone(),
    }
  }
}

impl DeserializedHeader {
  /// Create a new deserialized header.
  pub fn new(
    header_info: HeaderInfo,
    session_keys: Vec<Vec<u8>>,
    header_size: u64,
    edit_list: Option<Vec<u64>>,
  ) -> Self {
    Self {
      header_info,
      session_keys,
      header_size,
      edit_list,
    }
  }

  /// Grab all the required information from the header.
  /// This is more or less directly copied from https://github.com/EGA-archive/crypt4gh-rust/blob/2d41a1770067003bc67ab499841e0def186ed218/src/lib.rs#L283-L314
  pub fn from_buffer<R: Read>(read_buffer: &mut R, keys: &[Keys]) -> Result<Self, Crypt4GHError> {
    // Get header info
    let mut temp_buf = [0_u8; 16]; // Size of the header
    read_buffer
      .read_exact(&mut temp_buf)
      .map_err(|e| Crypt4GHError::ReadHeaderError(e.into()))?;
    let header_info: HeaderInfo = header::deconstruct_header_info(&temp_buf)?;

    let mut bytes = vec![];
    let mut header_lengths = 0;
    // Calculate header packets
    let encrypted_packets = (0..header_info.packets_count)
      .map(|_| {
        // Get length
        let mut length_buffer = [0_u8; 4];
        read_buffer
          .read_exact(&mut length_buffer)
          .map_err(|e| Crypt4GHError::ReadHeaderPacketLengthError(e.into()))?;

        bytes.extend(length_buffer);

        let length = bincode::deserialize::<u32>(&length_buffer)
          .map_err(|e| Crypt4GHError::ParseHeaderPacketLengthError(e))?;

        header_lengths += length;

        let length = length - 4;

        // Get data
        let mut encrypted_data = vec![0_u8; length as usize];
        read_buffer
          .read_exact(&mut encrypted_data)
          .map_err(|e| Crypt4GHError::ReadHeaderPacketDataError(e.into()))?;

        bytes.extend(encrypted_data.clone());

        Ok(encrypted_data)
      })
      .collect::<Result<Vec<Vec<u8>>, Crypt4GHError>>()?;

    let DecryptedHeaderPackets {
      data_enc_packets: session_keys,
      edit_list_packet,
    } = header::deconstruct_header_body(encrypted_packets, keys, &None)?;

    let header_size = 16 + header_lengths;

    Ok(DeserializedHeader::new(
      header_info,
      session_keys,
      header_size as u64,
      edit_list_packet,
    ))
  }

  /// Check if an edit list is present.
  pub fn contains_edit_list(&self) -> bool {
    self.edit_list.is_some()
  }
}

/// Represents the decrypted data from a C4GH file.
#[derive(Debug, Clone)]
pub struct DecryptedData(Vec<u8>);

impl DecryptedData {
  /// Decrypt the data from the header and read buffer. The read buffer is expected to be
  /// positioned at the start of the encrypted data.
  pub fn from_header<R: Read>(
    read_buffer: &mut R,
    header: DeserializedHeader,
  ) -> Result<Self, Crypt4GHError> {
    let mut writer = BufWriter::new(Cursor::new(vec![]));
    let mut write_info = WriteInfo::new(0, None, &mut writer);

    match header.edit_list {
      None => body_decrypt(read_buffer, &header.session_keys, &mut write_info, 0)?,
      Some(edit_list_content) => body_decrypt_parts(
        read_buffer,
        header.session_keys,
        write_info,
        edit_list_content,
      )?,
    }

    let data = writer
      .into_inner()
      .map_err(|err| Crypt4GHError::IoError(io::Error::other(err)))?
      .into_inner();

    Ok(Self(data))
  }

  /// Get the inner data.
  pub fn into_inner(self) -> Vec<u8> {
    self.0
  }
}

/// Convert an encrypted file position to an unencrypted position if the header length is known.
pub fn to_unencrypted(encrypted_position: u64, header_length: u64) -> u64 {
  if encrypted_position < header_length + NONCE_SIZE {
    return 0;
  }

  let number_data_blocks = encrypted_position / DATA_BLOCK_SIZE;
  let mut additional_bytes = number_data_blocks * (NONCE_SIZE + MAC_SIZE);

  let remainder = encrypted_position % DATA_BLOCK_SIZE;
  if remainder != 0 {
    additional_bytes += NONCE_SIZE;
  }

  encrypted_position - header_length - additional_bytes
}

/// Convert an encrypted file size to an unencrypted file size if the header length is known.
pub fn to_unencrypted_file_size(encrypted_file_size: u64, header_length: u64) -> u64 {
  if encrypted_file_size < header_length + NONCE_SIZE + MAC_SIZE {
    return 0;
  }

  to_unencrypted(encrypted_file_size, header_length) - MAC_SIZE
}

fn to_current_data_block(pos: u64, header_len: u64) -> u64 {
  header_len + (pos / ENCRYPTED_BLOCK_SIZE) * DATA_BLOCK_SIZE
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
    to_current_data_block(pos, header_len) + DATA_BLOCK_SIZE,
  )
}

fn unencrypted_clamped_position(pos: u64, encrypted_file_size: u64) -> u64 {
  let data_block_positions = unencrypted_to_data_block(pos, 0, encrypted_file_size);
  let data_block_count = data_block_positions / DATA_BLOCK_SIZE;

  data_block_positions - ((NONCE_SIZE + MAC_SIZE) * data_block_count)
}

/// Convert an unencrypted position to the additional bytes prior to the position that must be
/// included when encrypting data blocks.
pub fn unencrypted_clamp(pos: u64, header_length: u64, encrypted_file_size: u64) -> u64 {
  min(
    to_unencrypted_file_size(encrypted_file_size, header_length),
    unencrypted_clamped_position(pos, encrypted_file_size),
  )
}

/// Convert an unencrypted position to the additional bytes after to the position that must be
/// included when encrypting data blocks.
pub fn unencrypted_clamp_next(pos: u64, header_length: u64, encrypted_file_size: u64) -> u64 {
  min(
    to_unencrypted_file_size(encrypted_file_size, header_length),
    unencrypted_clamped_position(pos, encrypted_file_size) + ENCRYPTED_BLOCK_SIZE,
  )
}

/// Convert an unencrypted file size to an encrypted file size if the header length is known.
pub fn to_encrypted_file_size(file_size: u64, header_length: u64) -> u64 {
  to_encrypted(file_size, header_length) + MAC_SIZE
}

/// Convert an unencrypted file position to an encrypted position if the header length is known.
pub fn to_encrypted(position: u64, header_length: u64) -> u64 {
  let number_data_blocks = position / ENCRYPTED_BLOCK_SIZE;
  // Additional bytes include the full data block size.
  let mut additional_bytes = number_data_blocks * (NONCE_SIZE + MAC_SIZE);

  // If there is left over data, then there are more nonce bytes.
  let remainder = position % ENCRYPTED_BLOCK_SIZE;
  if remainder != 0 {
    additional_bytes += NONCE_SIZE;
  }

  // Then add the extra bytes to the current position.
  header_length + position + additional_bytes
}

#[cfg(test)]
mod tests {
  use super::*;

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
  fn test_to_unencrypted() {
    let result = to_unencrypted(124, 124);
    assert_eq!(result, 0);
    let result = to_unencrypted(124 + 12, 124);
    assert_eq!(result, 0);
    let result = to_unencrypted(124 + 12 + 12, 124);
    assert_eq!(result, 12);
  }

  #[test]
  fn test_to_unencrypted_file_size() {
    let result = to_unencrypted_file_size(124, 124);
    assert_eq!(result, 0);
    let result = to_unencrypted_file_size(124 + 12 + 16, 124);
    assert_eq!(result, 0);
    let result = to_unencrypted_file_size(124 + 12 + 16 + 12, 124);
    assert_eq!(result, 12);
  }

  #[test]
  fn test_unencrypted_clamp() {
    let pos = 0;
    let expected = 0;
    let result = unencrypted_clamp(pos, 0, to_encrypted_file_size(5485112, 0));
    assert_eq!(result, expected);

    let pos = 145110;
    let expected = 131072;
    let result = unencrypted_clamp(pos, 0, to_encrypted_file_size(5485112, 0));
    assert_eq!(result, expected);

    let pos = 5485074;
    let expected = 5439488;
    let result = unencrypted_clamp(pos, 0, to_encrypted_file_size(5485112, 0));
    assert_eq!(result, expected);
  }

  #[test]
  fn test_unencrypted_clamp_next() {
    let pos = 7853;
    let expected = 65536;
    let result = unencrypted_clamp_next(pos, 0, to_encrypted_file_size(5485112, 0));
    assert_eq!(result, expected);

    let pos = 453039;
    let expected = 458752;
    let result = unencrypted_clamp_next(pos, 0, to_encrypted_file_size(5485112, 0));
    assert_eq!(result, expected);

    let pos = 5485112;
    let expected = 5485112;
    let result = unencrypted_clamp_next(pos, 0, to_encrypted_file_size(5485112, 0));
    assert_eq!(result, expected);
  }
}
