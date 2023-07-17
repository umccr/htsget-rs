use bytes::{Bytes, BytesMut};
use crypt4gh::header::{deconstruct_header_info, HeaderInfo};
use std::io;
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

/// The type that a block is decoded into.
#[derive(Debug)]
pub enum BlockType {
  /// The magic string, version string and header packet count.
  /// Corresponds to `deconstruct_header_info`.
  HeaderInfo(HeaderInfo),
  /// The header packets, either data encryption key packets or data edit list packets.
  /// Corresponds to `deconstruct_header_body`.
  HeaderPacket(Bytes),
  /// The encrypted data blocks
  /// Corresponds to `body_decrypt`.
  DataBlock(Bytes),
}

impl BlockType {
  fn decode_header_info(src: &mut BytesMut) -> Result<Option<Self>, io::Error> {
    if src.len() < HEADER_INFO_SIZE {
      src.reserve(HEADER_INFO_SIZE);
      return Ok(None);
    }

    // todo: remove asserts within `deconstruct_header_info`
    Ok(Some(Self::HeaderInfo(
      deconstruct_header_info(src.split_to(HEADER_INFO_SIZE).as_ref().try_into().map_err(
        |err| {
          io::Error::new(
            io::ErrorKind::Other,
            format!("converting slice to fixed size array: {}", err),
          )
        },
      )?)
      .map_err(|err| {
        io::Error::new(
          io::ErrorKind::Other,
          format!("deconstructing header info: {}", err),
        )
      })?,
    )))
  }
}

/// State to keep track of the current block being decoded corresponding to `BlockType`.
#[derive(Debug)]
enum BlockState {
  HeaderInfo,
  HeaderPacket,
  DataBlock,
}

#[derive(Debug)]
pub struct Block {
  next_block: BlockState,
}

impl Block {
  pub fn new() -> Self {
    Self {
      next_block: BlockState::HeaderInfo,
    }
  }
}

impl Decoder for Block {
  type Item = BlockType;
  type Error = io::Error;

  fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
    match self.next_block {
      BlockState::HeaderInfo => BlockType::decode_header_info(src),
      BlockState::HeaderPacket => {
        todo!();
      }
      BlockState::DataBlock => {
        if src.len() < DATA_BLOCK_SIZE {
          src.reserve(DATA_BLOCK_SIZE);
          return Ok(None);
        }

        Ok(Some(BlockType::DataBlock(
          src.split_to(DATA_BLOCK_SIZE).freeze(),
        )))
      }
    }
  }
}
