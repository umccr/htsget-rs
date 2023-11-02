use crate::decoder::Block;
use std::cmp::min;

fn to_encrypted_file(pos: u64, header_len: u64) -> u64 {
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


/// Convert an unencrypted file size to an encrypted file size if the header length is known.
pub fn to_encrypted_file_size(file_size: u64, header_length: u64) -> u64 {
  to_encrypted_file(file_size, header_length) + Block::mac_size()
}

/// Convert an unencrypted position to an encrypted position as shown in
/// https://samtools.github.io/hts-specs/crypt4gh.pdf chapter 4.1.
pub fn current_data_block(pos: u64, header_len: u64, file_size: u64) -> u64 {
  min(file_size, to_encrypted_file(pos, header_len))
}

/// Get the next data block position from the unencrypted position.
pub fn next_data_block(pos: u64, header_len: u64, file_size: u64) -> u64 {
  min(
    file_size,
    to_encrypted_file(pos, header_len) + Block::standard_data_block_size(),
  )
}

#[cfg(test)]
mod tests {
  use crate::util::{current_data_block, next_data_block};

  #[test]
  fn test_to_encrypted() {
    let pos = 80000;
    let expected = 120 + 65536 + 12 + 16;
    let result = current_data_block(pos, 120, 100000);
    assert_eq!(result, expected);
  }

  #[test]
  fn test_to_encrypted_file_size() {
    let pos = 110000;
    let expected = 60000;
    let result = current_data_block(pos, 120, 60000);
    assert_eq!(result, expected);
  }

  #[test]
  fn test_to_encrypted_pos_greater_than_file_size() {
    let pos = 110000;
    let expected = 120 + 65536 + 12 + 16;
    let result = current_data_block(pos, 120, 100000);
    assert_eq!(result, expected);
  }

  #[test]
  fn test_next_data_block() {
    let pos = 100000;
    let expected = 120 + (65536 + 12 + 16) * 2;
    let result = next_data_block(pos, 120, 150000);
    assert_eq!(result, expected);
  }

  #[test]
  fn test_next_data_block_file_size() {
    let pos = 110000;
    let expected = 100000;
    let result = next_data_block(pos, 120, 100000);
    assert_eq!(result, expected);
  }
}
