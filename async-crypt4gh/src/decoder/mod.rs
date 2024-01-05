use std::io;

use bytes::{Bytes, BytesMut};
use crypt4gh::header::{deconstruct_header_info, HeaderInfo};
use tokio_util::codec::Decoder;

use crate::error::Error::{
  Crypt4GHError, DecodingHeaderInfo, MaximumHeaderSize, NumericConversionError,
  SliceConversionError,
};
use crate::error::{Error, Result};
use crate::{EncryptedHeaderPacketBytes, EncryptedHeaderPackets};

pub const ENCRYPTED_BLOCK_SIZE: usize = 65536;
pub const NONCE_SIZE: usize = 12; // ChaCha20 IETF Nonce size
pub const MAC_SIZE: usize = 16;

const DATA_BLOCK_SIZE: usize = NONCE_SIZE + ENCRYPTED_BLOCK_SIZE + MAC_SIZE;

const MAGIC_STRING_SIZE: usize = 8;
const VERSION_STRING_SIZE: usize = 4;
const HEADER_PACKET_COUNT_SIZE: usize = 4;

pub const HEADER_INFO_SIZE: usize =
  MAGIC_STRING_SIZE + VERSION_STRING_SIZE + HEADER_PACKET_COUNT_SIZE;

const HEADER_PACKET_LENGTH_SIZE: usize = 4;

/// Have some sort of maximum header size to prevent any overflows.
const MAX_HEADER_SIZE: usize = 8 * 1024 * 1024;

/// The type that a block is decoded into.
#[derive(Debug)]
pub enum DecodedBlock {
  /// The magic string, version string and header packet count.
  /// Corresponds to `deconstruct_header_info`.
  HeaderInfo(HeaderInfo),
  /// Header packets, both data encryption key packets and a data edit list packets.
  /// Corresponds to `deconstruct_header_body`.
  HeaderPackets(EncryptedHeaderPackets),
  /// The encrypted data blocks
  /// Corresponds to `body_decrypt`.
  DataBlock(Bytes),
}

/// State to keep track of the current block being decoded corresponding to `BlockType`.
#[derive(Debug)]
enum BlockState {
  /// Expecting header info.
  HeaderInfo,
  /// Expecting header packets and the number of header packets left to decode.
  HeaderPackets(u32),
  /// Expecting a data block.
  DataBlock,
  /// Expecting the end of the file. This is to account for the last data block potentially being
  /// shorter.
  Eof,
}

#[derive(Debug)]
pub struct Block {
  next_block: BlockState,
}

impl Block {
  fn get_header_info(src: &mut BytesMut) -> Result<HeaderInfo> {
    deconstruct_header_info(
      src
        .split_to(HEADER_INFO_SIZE)
        .as_ref()
        .try_into()
        .map_err(|_| SliceConversionError)?,
    )
    .map_err(DecodingHeaderInfo)
  }

  /// Parses the header info, updates the state and returns the block type. Unlike the other
  /// `decode` methods, this method parses the header info before returning a decoded block
  /// because the header info contains the number of packets which is required for decoding
  /// the rest of the source.
  pub fn decode_header_info(&mut self, src: &mut BytesMut) -> Result<Option<DecodedBlock>> {
    // Header info is a fixed size.
    if src.len() < HEADER_INFO_SIZE {
      src.reserve(HEADER_INFO_SIZE);
      return Ok(None);
    }

    // Parse the header info because it contains the number of header packets.
    let header_info = Self::get_header_info(src)?;

    self.next_block = BlockState::HeaderPackets(header_info.packets_count);

    Ok(Some(DecodedBlock::HeaderInfo(header_info)))
  }

  /// Decodes header packets, updates the state and returns a header packet block type.
  pub fn decode_header_packets(
    &mut self,
    src: &mut BytesMut,
    header_packets: u32,
  ) -> Result<Option<DecodedBlock>> {
    let mut header_packet_bytes = vec![];
    for _ in 0..header_packets {
      // Get enough bytes to read the header packet length.
      if src.len() < HEADER_PACKET_LENGTH_SIZE {
        src.reserve(HEADER_PACKET_LENGTH_SIZE);
        return Ok(None);
      }

      // Read the header packet length.
      let length_bytes = src.split_to(HEADER_PACKET_LENGTH_SIZE).freeze();
      let mut length: usize = u32::from_le_bytes(
        length_bytes
          .as_ref()
          .try_into()
          .map_err(|_| SliceConversionError)?,
      )
      .try_into()
      .map_err(|_| NumericConversionError)?;

      // We have already taken 4 bytes out of the length.
      length -= HEADER_PACKET_LENGTH_SIZE;

      // Have a maximum header size to prevent any overflows.
      if length > MAX_HEADER_SIZE {
        return Err(MaximumHeaderSize);
      }

      // Get enough bytes to read the entire header packet.
      if src.len() < length {
        src.reserve(length - src.len());
        return Ok(None);
      }

      header_packet_bytes.push(EncryptedHeaderPacketBytes::new(
        length_bytes,
        src.split_to(length).freeze(),
      ));
    }

    self.next_block = BlockState::DataBlock;

    let header_length = u64::try_from(
      header_packet_bytes
        .iter()
        .map(|packet| packet.packet_length().len() + packet.header().len())
        .sum::<usize>(),
    )
    .map_err(|_| NumericConversionError)?;

    Ok(Some(DecodedBlock::HeaderPackets(
      EncryptedHeaderPackets::new(header_packet_bytes, header_length),
    )))
  }

  /// Decodes data blocks, updates the state and returns a data block type.
  pub fn decode_data_block(&mut self, src: &mut BytesMut) -> Result<Option<DecodedBlock>> {
    // Data blocks are a fixed size, so we can return the
    // next data block without much processing.
    if src.len() < DATA_BLOCK_SIZE {
      src.reserve(DATA_BLOCK_SIZE);
      return Ok(None);
    }

    self.next_block = BlockState::DataBlock;

    Ok(Some(DecodedBlock::DataBlock(
      src.split_to(DATA_BLOCK_SIZE).freeze(),
    )))
  }

  /// Get the standard size of all non-ending data blocks.
  pub const fn standard_data_block_size() -> u64 {
    DATA_BLOCK_SIZE as u64
  }

  /// Get the size of the magic string, version and header packet count.
  pub const fn header_info_size() -> u64 {
    HEADER_INFO_SIZE as u64
  }

  /// Get the encrypted block size, without nonce and mac bytes.
  pub const fn encrypted_block_size() -> u64 {
    ENCRYPTED_BLOCK_SIZE as u64
  }

  /// Get the size of the nonce.
  pub const fn nonce_size() -> u64 {
    NONCE_SIZE as u64
  }

  /// Get the size of the mac.
  pub const fn mac_size() -> u64 {
    MAC_SIZE as u64
  }

  /// Get the maximum possible header size.
  pub const fn max_header_size() -> u64 {
    MAX_HEADER_SIZE as u64
  }
}

impl Default for Block {
  fn default() -> Self {
    Self {
      next_block: BlockState::HeaderInfo,
    }
  }
}

impl Decoder for Block {
  type Item = DecodedBlock;
  type Error = Error;

  fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
    match self.next_block {
      BlockState::HeaderInfo => self.decode_header_info(src),
      BlockState::HeaderPackets(header_packets) => self.decode_header_packets(src, header_packets),
      BlockState::DataBlock => self.decode_data_block(src),
      BlockState::Eof => Ok(None),
    }
  }

  fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>> {
    // Need a custom implementation of decode_eof because the last data block can be shorter.
    match self.decode(buf)? {
      Some(frame) => Ok(Some(frame)),
      None => {
        if buf.is_empty() {
          Ok(None)
        } else if let BlockState::DataBlock = self.next_block {
          // The last data block can be smaller than 64KiB.
          if buf.len() <= DATA_BLOCK_SIZE {
            self.next_block = BlockState::Eof;

            Ok(Some(DecodedBlock::DataBlock(buf.split().freeze())))
          } else {
            Err(Crypt4GHError(
              "the last data block is too large".to_string(),
            ))
          }
        } else {
          Err(io::Error::new(io::ErrorKind::Other, "bytes remaining on stream").into())
        }
      }
    }
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::io::Cursor;

  use crypt4gh::header::{deconstruct_header_body, DecryptedHeaderPackets};
  use crypt4gh::{body_decrypt, Keys, WriteInfo};
  use futures_util::stream::Skip;
  use futures_util::StreamExt;
  use tokio::fs::File;
  use tokio::io::AsyncReadExt;
  use tokio_util::codec::FramedRead;

  use htsget_test::crypt4gh::get_decryption_keys;
  use htsget_test::http_tests::get_test_file;

  use crate::tests::get_original_file;

  use super::*;

  #[tokio::test]
  async fn decode_header_info() {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let mut reader = FramedRead::new(src, Block::default());

    let header_info = reader.next().await.unwrap().unwrap();

    // Assert that the first block output is a header info with one packet.
    assert!(
      matches!(header_info, DecodedBlock::HeaderInfo(header_info) if header_info.packets_count == 1)
    );
  }

  #[tokio::test]
  async fn decode_header_packets() {
    let (recipient_private_key, sender_public_key, header_packet, _) =
      get_first_header_packet().await;
    let header = get_header_packets(recipient_private_key, sender_public_key, header_packet);

    assert_first_header_packet(header);

    // Todo handle case where there is more than one header packet.
  }

  #[tokio::test]
  async fn decode_data_block() {
    let (header, data_block) = get_data_block(0).await;

    let read_buf = Cursor::new(data_block.to_vec());
    let mut write_buf = Cursor::new(vec![]);
    let mut write_info = WriteInfo::new(0, None, &mut write_buf);

    body_decrypt(read_buf, &header.data_enc_packets, &mut write_info, 0).unwrap();

    let decrypted_bytes = write_buf.into_inner();

    assert_first_data_block(decrypted_bytes).await;
  }

  #[tokio::test]
  async fn decode_eof() {
    let (header, data_block) = get_data_block(39).await;

    let read_buf = Cursor::new(data_block.to_vec());
    let mut write_buf = Cursor::new(vec![]);
    let mut write_info = WriteInfo::new(0, None, &mut write_buf);

    body_decrypt(read_buf, &header.data_enc_packets, &mut write_info, 0).unwrap();

    let decrypted_bytes = write_buf.into_inner();

    assert_last_data_block(decrypted_bytes).await;
  }

  /// Assert that the first header packet is a data encryption key packet.
  pub(crate) fn assert_first_header_packet(header: DecryptedHeaderPackets) {
    assert_eq!(header.data_enc_packets.len(), 1);
    assert!(header.edit_list_packet.is_none());
  }

  /// Assert that the last data block is equal to the expected ending bytes of the original file.
  pub(crate) async fn assert_last_data_block(decrypted_bytes: Vec<u8>) {
    let mut original_file = get_test_file("bam/htsnexus_test_NA12878.bam").await;
    let mut original_bytes = vec![];
    original_file
      .read_to_end(&mut original_bytes)
      .await
      .unwrap();

    assert_eq!(
      decrypted_bytes,
      original_bytes
        .into_iter()
        .rev()
        .take(40895)
        .rev()
        .collect::<Vec<u8>>()
    );
  }

  /// Assert that the first data block is equal to the first 64KiB of the original file.
  pub(crate) async fn assert_first_data_block(decrypted_bytes: Vec<u8>) {
    let original_bytes = get_original_file().await;

    assert_eq!(decrypted_bytes, original_bytes[..65536]);
  }

  /// Get the first header packet from the test file.
  pub(crate) async fn get_first_header_packet(
  ) -> (Keys, Vec<u8>, Vec<Bytes>, Skip<FramedRead<File, Block>>) {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_decryption_keys().await;

    let mut reader = FramedRead::new(src, Block::default()).skip(1);

    // The second block should contain a header packet.
    let header_packets = reader.next().await.unwrap().unwrap();

    let (header_packet, header_length) =
      if let DecodedBlock::HeaderPackets(header_packets) = header_packets {
        Some(header_packets)
      } else {
        None
      }
      .unwrap()
      .into_inner();

    assert_eq!(header_length, 108);

    (
      recipient_private_key,
      sender_public_key,
      header_packet
        .into_iter()
        .map(|packet| packet.into_header_bytes())
        .collect(),
      reader,
    )
  }

  /// Get the first data block from the test file.
  pub(crate) async fn get_data_block(skip: usize) -> (DecryptedHeaderPackets, Bytes) {
    let (recipient_private_key, sender_public_key, header_packets, reader) =
      get_first_header_packet().await;
    let header = get_header_packets(recipient_private_key, sender_public_key, header_packets);

    let data_block = reader.skip(skip).next().await.unwrap().unwrap();

    let data_block = if let DecodedBlock::DataBlock(data_block) = data_block {
      Some(data_block)
    } else {
      None
    }
    .unwrap();

    (header, data_block)
  }

  /// Get the header packets from a decoded block.
  pub(crate) fn get_header_packets(
    recipient_private_key: Keys,
    sender_public_key: Vec<u8>,
    header_packets: Vec<Bytes>,
  ) -> DecryptedHeaderPackets {
    // Assert the size of the header packet is correct.
    assert_eq!(header_packets.len(), 1);
    assert_eq!(header_packets.first().unwrap().len(), 104);

    deconstruct_header_body(
      header_packets
        .into_iter()
        .map(|header_packet| header_packet.to_vec())
        .collect(),
      &[recipient_private_key],
      &Some(sender_public_key),
    )
    .unwrap()
  }
}
