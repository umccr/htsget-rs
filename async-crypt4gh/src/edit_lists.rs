use crate::error::{Error, Result};
use crate::reader::Reader;
use crate::util::{unencrypted_clamp, unencrypted_clamp_next};
use crate::PublicKey;
use crypt4gh::header::{encrypt, make_packet_data_edit_list, HeaderInfo};
use crypt4gh::Keys;
use rustls::PrivateKey;
use std::collections::HashSet;
use tokio::io::AsyncRead;

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

/// Add edit lists to the header packet.
pub async fn add_edit_list<R: AsyncRead + Unpin>(
  reader: &mut Reader<R>,
  unencrypted_positions: Vec<UnencryptedPosition>,
  private_key: PrivateKey,
  recipient_public_key: PublicKey,
  stream_length: u64,
) -> Result<Option<Vec<u8>>> {
  if reader.edit_list_packet().is_some() {
    return Err(Error::Crypt4GHError("edit lists already exist".to_string()));
  }

  // Todo, header info should have copy or clone on it.
  let (mut header_info, encrypted_header_packets) =
    if let (Some(header_info), Some(encrypted_header_packets)) =
      (reader.header_info(), reader.encrypted_header_packets())
    {
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

  let keys = Keys {
    method: 0,
    privkey: private_key.0,
    recipient_pubkey: recipient_public_key.into_inner(),
  };
  let edit_list = create_edit_list(unencrypted_positions, stream_length);
  let edit_list_packet =
    make_packet_data_edit_list(edit_list.into_iter().map(|edit| edit as usize).collect());
  let edit_list_bytes = encrypt(&edit_list_packet, &HashSet::from_iter(vec![keys]))?
    .into_iter()
    .last()
    .ok_or_else(|| Error::Crypt4GHError("could not encrypt header packet".to_string()))?;
  let edit_list_bytes = [
    ((edit_list_bytes.len() + 4) as u32).to_le_bytes().to_vec(),
    edit_list_bytes,
  ]
  .concat();

  let header = [
    header_info_bytes.as_slice(),
    encrypted_header_packets.as_slice(),
    edit_list_bytes.as_slice(),
  ]
  .concat();

  Ok(Some(header))
}

/// Create the edit lists from the unencrypted byte positions.
pub fn create_edit_list(
  unencrypted_positions: Vec<UnencryptedPosition>,
  stream_length: u64,
) -> Vec<u64> {
  let ranges_size = unencrypted_positions.len();
  let (edit_list, _) = unencrypted_positions.into_iter().fold(
    (Vec::with_capacity(ranges_size), 0),
    |(mut edit_list, previous_discard), range| {
      // Note, edit lists do not relate to the length of the crypt4gh header, only to the 65536 byte
      // boundaries of the encrypted blocks, so the boundaries can be treated like they have a 0 byte
      // size header.
      let start_boundary = unencrypted_clamp(range.start, stream_length);
      let end_boundary = unencrypted_clamp_next(range.end, stream_length);

      let discard = range.start - start_boundary + previous_discard;
      let keep = range.end - range.start;

      edit_list.extend([discard, keep]);
      (edit_list, end_boundary - range.end)
    },
  );
  edit_list
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::reader::builder::Builder;
  use crate::tests::{get_decryption_keys, get_encryption_keys};
  use htsget_test::http_tests::get_test_file;

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

    let header = add_edit_list(
      &mut reader,
      test_positions(),
      PrivateKey(private_key_encrypt.clone().privkey),
      PublicKey {
        bytes: public_key_encrypt.clone(),
      },
      5485112,
    )
    .await
    .unwrap()
    .unwrap();

    let mut reader = Builder::default()
      .with_sender_pubkey(PublicKey::new(public_key_decrypt))
      .with_stream_length(5485112)
      .build_with_reader(header.as_slice(), vec![private_key_decrypt]);
    reader.read_header().await.unwrap();

    let data_packets = reader.session_keys();
    assert_eq!(data_packets, expected_data_packets);

    let edit_lists = reader.edit_list_packet().unwrap();
    assert_eq!(edit_lists, expected_edit_list());
  }

  #[test]
  fn test_create_edit_list() {
    let edit_list = create_edit_list(test_positions(), 5485112);
    assert_eq!(edit_list, expected_edit_list());
  }

  fn test_positions() -> Vec<UnencryptedPosition> {
    vec![
      UnencryptedPosition::new(0, 7853),
      UnencryptedPosition::new(145110, 453039),
      UnencryptedPosition::new(5485074, 5485112),
    ]
  }

  fn expected_edit_list() -> Vec<u64> {
    vec![0, 7853, 71721, 307929, 51299, 38]
  }
}
