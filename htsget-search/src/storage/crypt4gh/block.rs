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
  fn decode_header_info(src: &mut BytesMut) -> Result<HeaderInfo> {
    deconstruct_header_info(
      src
        .split_to(HEADER_INFO_SIZE)
        .as_ref()
        .try_into()
        .map_err(SliceConversionError)?,
    )
    .map_err(DecodingHeaderInfo)
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
      BlockState::HeaderInfo => {
        if src.len() < HEADER_INFO_SIZE {
          src.reserve(HEADER_INFO_SIZE);
          return Ok(None);
        }

        let header_info = Self::decode_header_info(src)?;

        self.next_block = BlockState::HeaderPackets(header_info.packets_count);

        Ok(Some(BlockType::HeaderInfo(header_info)))
      }
      BlockState::HeaderPackets(mut header_packets) => {
        // Get enough bytes to read the header packet length.
        if src.len() < HEADER_PACKET_LENGTH_SIZE {
          src.reserve(HEADER_PACKET_LENGTH_SIZE);
          return Ok(None);
        }

        // Read the header packet length.
        let length: usize =
          u32::from_le_bytes(src.as_ref().try_into().map_err(SliceConversionError)?)
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
      BlockState::DataBlock => {
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
  }
}
