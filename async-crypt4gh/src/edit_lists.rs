use std::collections::HashSet;

use crypt4gh::header::{encrypt, make_packet_data_edit_list, HeaderInfo};
use crypt4gh::Keys;
use rustls::PrivateKey;
use tokio::io::AsyncRead;

use crate::error::{Error, Result};
use crate::reader::Reader;
use crate::PublicKey;

/// Unencrypted byte range positions. Contains inclusive start values and exclusive end values.
#[derive(Debug, Clone)]
pub struct UnencryptedPosition {
  start: u64,
  end: u64,
}

impl UnencryptedPosition {
  pub fn new(start: u64, end: u64) -> Self {
    Self { start, end }
  }

  pub fn start(&self) -> u64 {
    self.start
  }

  pub fn end(&self) -> u64 {
    self.end
  }
}

/// Encrypted byte range positions. Contains inclusive start values and exclusive end values.
#[derive(Debug, Clone)]
pub struct ClampedPosition {
  start: u64,
  end: u64,
}

impl ClampedPosition {
  pub fn new(start: u64, end: u64) -> Self {
    Self { start, end }
  }

  pub fn start(&self) -> u64 {
    self.start
  }

  pub fn end(&self) -> u64 {
    self.end
  }
}

/// Bytes representing a header packet with an edit list.
#[derive(Debug, Clone)]
pub struct Header {
  header_info: Vec<u8>,
  original_header: Vec<u8>,
  edit_list_packet: Vec<u8>,
}

impl Header {
  pub fn new(header_info: Vec<u8>, original_header: Vec<u8>, edit_list_packet: Vec<u8>) -> Self {
    Self {
      header_info,
      original_header,
      edit_list_packet,
    }
  }

  pub fn into_inner(self) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    (
      self.header_info,
      self.original_header,
      self.edit_list_packet,
    )
  }

  pub fn as_slice(&self) -> Vec<u8> {
    [
      self.header_info.as_slice(),
      self.original_header.as_slice(),
      self.edit_list_packet.as_slice(),
    ]
    .concat()
  }
}

impl From<(Vec<u8>, Vec<u8>, Vec<u8>)> for Header {
  fn from((header_info, original_header, edit_list_packet): (Vec<u8>, Vec<u8>, Vec<u8>)) -> Self {
    Self::new(header_info, original_header, edit_list_packet)
  }
}

pub struct EditHeader<'a, R>
where
  R: AsyncRead + Unpin,
{
  reader: &'a Reader<R>,
  unencrypted_positions: Vec<UnencryptedPosition>,
  clamped_positions: Vec<ClampedPosition>,
  private_key: PrivateKey,
  recipient_public_key: PublicKey,
}

impl<'a, R> EditHeader<'a, R>
where
  R: AsyncRead + Unpin,
{
  pub fn new(
    reader: &'a Reader<R>,
    unencrypted_positions: Vec<UnencryptedPosition>,
    clamped_positions: Vec<ClampedPosition>,
    private_key: PrivateKey,
    recipient_public_key: PublicKey,
  ) -> Self {
    Self {
      reader,
      unencrypted_positions,
      clamped_positions,
      private_key,
      recipient_public_key,
    }
  }

  /// Encrypt the edit list packet.
  pub fn encrypt_edit_list(&self, edit_list_packet: Vec<u8>) -> Result<Vec<u8>> {
    let keys = Keys {
      method: 0,
      privkey: self.private_key.clone().0,
      recipient_pubkey: self.recipient_public_key.clone().into_inner(),
    };

    encrypt(&edit_list_packet, &HashSet::from_iter(vec![keys]))?
      .into_iter()
      .last()
      .ok_or_else(|| Error::Crypt4GHError("could not encrypt header packet".to_string()))
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
  pub fn edit_list(self) -> Result<Option<Header>> {
    if self.reader.edit_list_packet().is_some() {
      return Err(Error::Crypt4GHError("edit lists already exist".to_string()));
    }

    // Todo, header info should have copy or clone on it.
    let (mut header_info, encrypted_header_packets) =
      if let (Some(header_info), Some(encrypted_header_packets)) = (
        self.reader.header_info(),
        self.reader.encrypted_header_packets(),
      ) {
        (
          HeaderInfo {
            magic_number: header_info.magic_number,
            version: header_info.version,
            packets_count: header_info.packets_count,
          },
          encrypted_header_packets
            .iter()
            .flat_map(|packet| [packet.packet_length().to_vec(), packet.header.to_vec()].concat())
            .collect::<Vec<u8>>(),
        )
      } else {
        return Ok(None);
      };

    // Todo rewrite this from the context of an encryption stream like the decrypter.
    header_info.packets_count += 1;
    let header_info_bytes =
      bincode::serialize(&header_info).map_err(|err| Error::Crypt4GHError(err.to_string()))?;

    let edit_list = self.create_edit_list();
    let edit_list_packet =
      make_packet_data_edit_list(edit_list.into_iter().map(|edit| edit as usize).collect());

    let edit_list_bytes = self.encrypt_edit_list(edit_list_packet)?;
    let edit_list_bytes = [
      ((edit_list_bytes.len() + 4) as u32).to_le_bytes().to_vec(),
      edit_list_bytes,
    ]
    .concat();

    Ok(Some(
      (header_info_bytes, encrypted_header_packets, edit_list_bytes).into(),
    ))
  }
}

#[cfg(test)]
mod tests {
  use htsget_test::crypt4gh::{get_decryption_keys, get_encryption_keys};
  use htsget_test::http_tests::get_test_file;

  use crate::reader::builder::Builder;

  use super::*;

  #[tokio::test]
  async fn test_append_edit_list() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (private_key_decrypt, public_key_decrypt) = get_decryption_keys().await;
    let (private_key_encrypt, public_key_encrypt) = get_encryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(public_key_decrypt.clone()))
      .with_stream_length(5485112)
      .build_with_reader(src, vec![private_key_decrypt.clone()]);
    reader.read_header().await.unwrap();

    let expected_data_packets = reader.session_keys().to_vec();

    let header = EditHeader::new(
      &reader,
      test_unencrypted_positions(),
      test_clamped_positions(),
      PrivateKey(private_key_encrypt.clone().privkey),
      PublicKey {
        bytes: public_key_encrypt.clone(),
      },
    )
    .edit_list()
    .unwrap()
    .unwrap();

    let header_slice = header.as_slice();
    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(public_key_decrypt))
      .with_stream_length(5485112)
      .build_with_reader(header_slice.as_slice(), vec![private_key_decrypt]);
    reader.read_header().await.unwrap();

    let data_packets = reader.session_keys();
    assert_eq!(data_packets, expected_data_packets);

    let edit_lists = reader.edit_list_packet().unwrap();
    assert_eq!(edit_lists, expected_edit_list());
  }

  #[tokio::test]
  async fn test_create_edit_list() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (private_key_decrypt, public_key_decrypt) = get_decryption_keys().await;
    let (private_key_encrypt, public_key_encrypt) = get_encryption_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(public_key_decrypt.clone()))
      .with_stream_length(5485112)
      .build_with_reader(src, vec![private_key_decrypt.clone()]);
    reader.read_header().await.unwrap();

    let edit_list = EditHeader::new(
      &reader,
      test_unencrypted_positions(),
      test_clamped_positions(),
      PrivateKey(private_key_encrypt.clone().privkey),
      PublicKey {
        bytes: public_key_encrypt.clone(),
      },
    )
    .create_edit_list();

    assert_eq!(edit_list, expected_edit_list());
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
