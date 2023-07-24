use crate::storage::crypt4gh::error::Error::{
  DecodingHeaderInfo, MaximumHeaderSize, NumericConversionError, SliceConversionError,
};
use crate::storage::crypt4gh::error::{Error, Result};
use bytes::{Bytes, BytesMut};
use crypt4gh::header::{deconstruct_header_info, HeaderInfo};
use tokio_util::codec::Decoder;

pub const ENCRYPTED_BLOCK_SIZE: usize = 65536;
pub const NONCE_SIZE: usize = 12; // ChaCha20 IETF Nonce size
pub const MAC_SIZE: usize = 16;

pub const DATA_BLOCK_SIZE: usize = NONCE_SIZE + ENCRYPTED_BLOCK_SIZE + MAC_SIZE;

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
  /// A header packet, either a data encryption key packet or a data edit list packet.
  /// Corresponds to `deconstruct_header_body`.
  HeaderPacket(Bytes),
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
    mut header_packets: u32,
  ) -> Result<Option<DecodedBlock>> {
    // Get enough bytes to read the header packet length.
    if src.len() < HEADER_PACKET_LENGTH_SIZE {
      src.reserve(HEADER_PACKET_LENGTH_SIZE);
      return Ok(None);
    }

    // Read the header packet length.
    let mut length: usize = u32::from_le_bytes(
      src
        .split_to(HEADER_PACKET_LENGTH_SIZE)
        .freeze()
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

    // Keep processing header packets if there are any left,
    // otherwise go to data blocks.
    header_packets -= 1;
    if header_packets > 0 {
      self.next_block = BlockState::HeaderPackets(header_packets);
    } else {
      self.next_block = BlockState::DataBlock;
    }

    Ok(Some(DecodedBlock::HeaderPacket(
      src.split_to(length).freeze(),
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
    }
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;
  use crypt4gh::header::{deconstruct_header_body, DecryptedHeaderPackets};
  use crypt4gh::{body_decrypt, Keys, WriteInfo};
  use std::io::Cursor;

  use crate::storage::crypt4gh::tests::get_keys;
  use futures_util::StreamExt;
  use htsget_test::http_tests::get_test_file;
  use tokio::io::AsyncReadExt;
  use tokio_util::codec::FramedRead;

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
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let reader = FramedRead::new(src, Block::default());

    // The second block should contain a header packet.
    let header_packet = reader.skip(1).next().await.unwrap().unwrap();
    let header = get_header_packets(recipient_private_key, sender_public_key, header_packet);

    // Assert that the header packet contains only one data encryption key packet.
    assert_eq!(header.data_enc_packets.len(), 1);
    assert!(header.edit_list_packet.is_none());

    // Todo handle case where there is more than one header packet.
  }

  #[tokio::test]
  async fn decode_data_block() {
    let (header, data_block) = get_first_data_block().await;

    let read_buf = Cursor::new(data_block.to_vec());
    let mut write_buf = Cursor::new(vec![]);
    let mut write_info = WriteInfo::new(0, None, &mut write_buf);

    body_decrypt(read_buf, &header.data_enc_packets, &mut write_info, 0).unwrap();

    let decrypted_bytes = write_buf.into_inner();

    assert_first_data_block(decrypted_bytes).await;
  }

  /// Assert that the first data block is equal to the first 64KiB of the original file.
  pub(crate) async fn assert_first_data_block(decrypted_bytes: Vec<u8>) {
    let mut original_file = get_test_file("bam/htsnexus_test_NA12878.bam").await;
    let mut original_bytes = [0u8; 65536];
    original_file.read_exact(&mut original_bytes).await.unwrap();

    assert_eq!(decrypted_bytes, original_bytes);
  }

  /// Get the first data block from the test file.
  pub(crate) async fn get_first_data_block() -> (DecryptedHeaderPackets, Bytes) {
    let src = get_test_file("crypt4gh/htsnexus_test_NA12878.bam.c4gh").await;
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut reader = FramedRead::new(src, Block::default()).skip(1);

    let header_packet = reader.next().await.unwrap().unwrap();
    let header = get_header_packets(recipient_private_key, sender_public_key, header_packet);

    // The third block should be a data block.
    let data_block = reader.next().await.unwrap().unwrap();

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
    header_packet: DecodedBlock,
  ) -> DecryptedHeaderPackets {
    let header_packet = if let DecodedBlock::HeaderPacket(header_packet) = header_packet {
      Some(header_packet)
    } else {
      None
    }
    .unwrap();

    // Assert the size of the header packet is correct.
    assert_eq!(header_packet.len(), 104);

    deconstruct_header_body(
      vec![header_packet.to_vec()],
      &[recipient_private_key],
      &Some(sender_public_key),
    )
    .unwrap()
  }
}
