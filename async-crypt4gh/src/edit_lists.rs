use crate::decoder::HEADER_INFO_SIZE;
use crate::error::{Error, Result};
use crate::util::{unencrypted_to_data_block, unencrypted_to_next_data_block};
use crate::PublicKey;
use crypt4gh::header::{deconstruct_header_info, encrypt, make_packet_data_edit_list};
use crypt4gh::Keys;
use rustls::PrivateKey;
use std::array::TryFromSliceError;
use std::collections::HashSet;

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
pub async fn append_edit_list(
  mut header: Vec<u8>,
  unencrypted_positions: Vec<UnencryptedPosition>,
  private_key: PrivateKey,
  recipient_public_key: PublicKey,
  stream_length: u64,
) -> Result<Vec<u8>> {
  let keys = Keys {
    method: 0,
    privkey: private_key.0,
    recipient_pubkey: recipient_public_key.into_inner(),
  };

  let edit_list = create_edit_list(unencrypted_positions, stream_length);

  // Todo rewrite this from the context of an encryption stream like the decrypter.
  let edit_list_packet =
    make_packet_data_edit_list(edit_list.into_iter().map(|edit| edit as usize).collect());
  let header_bytes = encrypt(&edit_list_packet, &HashSet::from_iter(vec![keys]))?
    .into_iter()
    .last()
    .ok_or_else(|| Error::Crypt4GHError("could not encrypt header packet".to_string()))?;

  header.extend(header_bytes);
  let mut header_info = deconstruct_header_info(
    &header[..HEADER_INFO_SIZE]
      .try_into()
      .map_err(|err: TryFromSliceError| Error::Crypt4GHError(err.to_string()))?,
  )?;
  header_info.packets_count += 1;

  let mut header_bytes =
    bincode::serialize(&header_info).map_err(|err| Error::Crypt4GHError(err.to_string()))?;
  header_bytes.extend(&header[HEADER_INFO_SIZE..]);

  Ok(header_bytes)
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
      // boundaries of the data packets, so the boundaries can be treated like they have a 0 byte
      // size header.
      let start_boundary = unencrypted_to_data_block(range.start, 0, stream_length);
      let end_boundary = unencrypted_to_next_data_block(range.end, 0, stream_length);

      let discard = range.start - start_boundary + previous_discard;
      let keep = range.end - range.start;

      edit_list.extend([discard, keep]);
      (edit_list, end_boundary - range.end)
    },
  );
  edit_list
}
