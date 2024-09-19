//! Edit list functionality.
//!

use crate::c4gh::DeserializedHeader;
use crate::error::{Result, StorageError};
use crypt4gh::error::Crypt4GHError;
use crypt4gh::error::Crypt4GHError::InvalidPacketType;
use crypt4gh::header::{encrypt, make_packet_data_edit_list, make_packet_data_enc, HeaderInfo};
use crypt4gh::Keys;
use std::collections::HashSet;
use tokio::io;

/// Unencrypted byte range positions. Contains inclusive start values and exclusive end values.
#[derive(Debug, Clone)]
pub struct UnencryptedPosition {
  start: u64,
  end: u64,
}

impl UnencryptedPosition {
  /// Create new positions.
  pub fn new(start: u64, end: u64) -> Self {
    Self { start, end }
  }
}

/// Encrypted byte range positions. Contains inclusive start values and exclusive end values.
#[derive(Debug, Clone)]
pub struct ClampedPosition {
  start: u64,
  end: u64,
}

impl ClampedPosition {
  /// Create new positions.
  pub fn new(start: u64, end: u64) -> Self {
    Self { start, end }
  }
}

/// Bytes representing a header packet with an edit list.
#[derive(Debug, Clone)]
pub struct Header {
  header_info: Vec<u8>,
  data_enc_packets: Vec<u8>,
  edit_list_packet: Vec<u8>,
}

impl Header {
  /// Create a new header.
  pub fn new(header_info: Vec<u8>, data_enc_packets: Vec<u8>, edit_list_packet: Vec<u8>) -> Self {
    Self {
      header_info,
      data_enc_packets,
      edit_list_packet,
    }
  }

  /// Get the inner values.
  pub fn into_inner(self) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    (
      self.header_info,
      self.data_enc_packets,
      self.edit_list_packet,
    )
  }
}

impl From<(Vec<u8>, Vec<u8>, Vec<u8>)> for Header {
  fn from((header_info, data_enc_packets, edit_list_packet): (Vec<u8>, Vec<u8>, Vec<u8>)) -> Self {
    Self::new(header_info, data_enc_packets, edit_list_packet)
  }
}

/// The edit header struct creates and updates C4GH headers with edit lists.
pub struct EditHeader<'a> {
  unencrypted_positions: Vec<UnencryptedPosition>,
  clamped_positions: Vec<ClampedPosition>,
  keys: &'a [Keys],
  current_header: &'a DeserializedHeader,
}

impl<'a> EditHeader<'a> {
  /// Create a new edit header.
  pub fn new(
    unencrypted_positions: Vec<UnencryptedPosition>,
    clamped_positions: Vec<ClampedPosition>,
    keys: &'a [Keys],
    current_header: &'a DeserializedHeader,
  ) -> Self {
    Self {
      unencrypted_positions,
      clamped_positions,
      keys,
      current_header,
    }
  }

  /// Encrypt the header packet.
  pub fn encrypt_header_packet(&self, header_packet: Vec<u8>) -> Result<Vec<u8>> {
    Ok(
      encrypt(&header_packet, &HashSet::from_iter(self.keys.to_vec()))?
        .into_iter()
        .last()
        .ok_or_else(|| {
          Crypt4GHError::UnableToEncryptPacket("could not encrypt header packet".to_string())
        })?,
    )
  }

  /// Create the edit lists from the unencrypted byte positions.
  pub fn create_edit_list(&self) -> Vec<u64> {
    let mut unencrypted_positions: Vec<u64> = self
      .unencrypted_positions
      .iter()
      .flat_map(|pos| [pos.start, pos.end])
      .collect();

    // Collect the clamped and unencrypted positions into separate edit list groups.
    let (mut edit_list, last_discard) =
      self
        .clamped_positions
        .iter()
        .fold((vec![], 0), |(mut edit_list, previous_discard), pos| {
          // Get the correct number of unencrypted positions that fit within this clamped position.
          let partition =
            unencrypted_positions.partition_point(|unencrypted_pos| unencrypted_pos <= &pos.end);
          let mut positions: Vec<u64> = unencrypted_positions.drain(..partition).collect();

          // Merge all positions.
          positions.insert(0, pos.start);
          positions.push(pos.end);

          // Find the difference between consecutive positions to get the edits.
          let mut positions: Vec<u64> = positions
            .iter()
            .zip(positions.iter().skip(1))
            .map(|(start, end)| end - start)
            .collect();

          // Add the previous discard to the first edit.
          if let Some(first) = positions.first_mut() {
            *first += previous_discard;
          }

          // If the last edit is a discard, then carry this over into the next iteration.
          let next_discard = if positions.len() % 2 == 0 {
            0
          } else {
            positions.pop().unwrap_or(0)
          };

          // Add edits to the accumulating edit list.
          edit_list.extend(positions);
          (edit_list, next_discard)
        });

    // If there is a final discard, then add this to the edit list.
    if last_discard != 0 {
      edit_list.push(last_discard);
    }

    edit_list
  }

  /// Add edit lists and return a header packet.
  pub fn reencrypt_header(self) -> Result<Header> {
    if self.current_header.contains_edit_list {
      return Err(StorageError::IoError(
        "edit lists already exist".to_string(),
        io::Error::other(Crypt4GHError::TooManyEditListPackets),
      ));
    }

    let edit_list = self.create_edit_list();
    let edit_list_packet =
      make_packet_data_edit_list(edit_list.into_iter().map(|edit| edit as usize).collect());

    let edit_list_bytes = self.encrypt_header_packet(edit_list_packet)?;
    let edit_list_bytes = [
      ((edit_list_bytes.len() + 4) as u32).to_le_bytes().to_vec(),
      edit_list_bytes,
    ]
    .concat();

    let mut header_packets = vec![];
    for session_key in self.current_header.session_keys.as_slice() {
      let data_enc_packet = make_packet_data_enc(
        0,
        session_key
          .as_slice()
          .try_into()
          .map_err(|_| Crypt4GHError::NoValidHeaderPacket)?,
      );
      let header_packet = self.encrypt_header_packet(data_enc_packet)?;
      header_packets.push(
        [
          ((header_packet.len() + 4) as u32).to_le_bytes().to_vec(),
          header_packet,
        ]
        .concat(),
      )
    }

    let header_info = &self.current_header.header_info;

    let mut current_len = header_info.packets_count;
    current_len += 1 + header_packets.len() as u32;

    let header_info = HeaderInfo {
      magic_number: header_info.magic_number,
      version: header_info.version,
      packets_count: current_len,
    };

    let header_info_bytes = bincode::serialize(&header_info).map_err(|_| InvalidPacketType)?;

    Ok(
      (
        header_info_bytes,
        header_packets.into_iter().flatten().collect(),
        edit_list_bytes,
      )
        .into(),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use htsget_test::c4gh::get_decryption_keys;
  use htsget_test::util::default_dir;
  use std::fs::File;
  use std::io::{BufReader, Cursor, Read};

  #[tokio::test]
  async fn test_create_edit_list() {
    let mut src =
      File::open(default_dir().join("data/c4gh/htsnexus_test_NA12878.bam.c4gh")).unwrap();
    let mut buf = vec![];
    src.read_to_end(&mut buf).unwrap();

    let mut buf = BufReader::new(Cursor::new(buf));
    let keys = get_decryption_keys();

    let edit = EditHeader::new(
      test_unencrypted_positions(),
      test_clamped_positions(),
      &keys,
      &DeserializedHeader::from_buffer(&mut buf, &keys).unwrap(),
    )
    .create_edit_list();

    assert_eq!(edit, expected_edit_list());
  }

  fn test_unencrypted_positions() -> Vec<UnencryptedPosition> {
    vec![
      UnencryptedPosition::new(0, 7853),
      UnencryptedPosition::new(145110, 453039),
      UnencryptedPosition::new(5485074, 5485112),
    ]
  }

  fn test_clamped_positions() -> Vec<ClampedPosition> {
    vec![
      ClampedPosition::new(0, 65536),
      ClampedPosition::new(131072, 458752),
      ClampedPosition::new(5439488, 5485112),
    ]
  }

  fn expected_edit_list() -> Vec<u64> {
    vec![0, 7853, 71721, 307929, 51299, 38]
  }
}
