use super::error::{Error, Result};
use crate::storage::crypt4gh::error::Error::{
  DecodingHeaderInfo, MaximumHeaderSize, NumericConversionError, SliceConversionError,
};
use bytes::{Bytes, BytesMut};
use crypt4gh::header::{deconstruct_header_info, HeaderInfo};
use tokio_util::codec::Decoder;

pub const ENCRYPTED_BLOCK_SIZE: usize = 65535;
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
pub enum BlockType {
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
        .map_err(SliceConversionError)?,
    )
    .map_err(DecodingHeaderInfo)
  }

  /// Parses the header info, updates the state and returns the block type.
  pub fn decode_header_info(&mut self, src: &mut BytesMut) -> Result<Option<BlockType>> {
    // Header info is a fixed size.
    if src.len() < HEADER_INFO_SIZE {
      src.reserve(HEADER_INFO_SIZE);
      return Ok(None);
    }

    // Parse the header info because it contains the number of header packets.
    let header_info = Self::get_header_info(src)?;

    self.next_block = BlockState::HeaderPackets(header_info.packets_count);

    Ok(Some(BlockType::HeaderInfo(header_info)))
  }

  /// Decodes header packets, updates the state and returns a header packet block type.
  pub fn decode_header_packets(
    &mut self,
    src: &mut BytesMut,
    mut header_packets: u32,
  ) -> Result<Option<BlockType>> {
    // Get enough bytes to read the header packet length.
    if src.len() < HEADER_PACKET_LENGTH_SIZE {
      src.reserve(HEADER_PACKET_LENGTH_SIZE);
      return Ok(None);
    }

    // Read the header packet length.
    let length: usize = u32::from_le_bytes(src.as_ref().try_into().map_err(SliceConversionError)?)
      .try_into()
      .map_err(NumericConversionError)?;

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

    Ok(Some(BlockType::HeaderPacket(src.split_to(length).freeze())))
  }

  /// Decodes data blocks, updates the state and returns a data block type.
  pub fn decode_data_block(&mut self, src: &mut BytesMut) -> Result<Option<BlockType>> {
    // Data blocks are a fixed size, so we can return the
    // next data block without much processing.
    if src.len() < DATA_BLOCK_SIZE {
      src.reserve(DATA_BLOCK_SIZE);
      return Ok(None);
    }

    self.next_block = BlockState::DataBlock;

    Ok(Some(BlockType::DataBlock(
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
  type Item = BlockType;
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
mod tests {
  use super::*;
  use futures_util::StreamExt;
  use tokio::fs::File;
  use tokio_util::codec::FramedRead;

  #[tokio::test]
  async fn decode_header_info() {
    let src = read_crypt4gh_file("htsnexus_test_NA12878.bam.c4gh").await;
    let mut reader = FramedRead::new(src, Block::default());

    let header_info = reader.next().await.unwrap().unwrap();

    // Assert that the first block output is a header info with one packet.
    assert!(
      matches!(header_info, BlockType::HeaderInfo(header_info) if header_info.packets_count == 1)
    );
  }

  pub async fn read_crypt4gh_file(file_name: &str) -> File {
    File::open(
      std::env::current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("data")
        .join("crypt4gh")
        .join(file_name),
    )
    .await
    .unwrap()
  }
}
