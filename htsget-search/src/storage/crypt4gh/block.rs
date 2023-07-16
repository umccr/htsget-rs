use bytes::{Bytes, BytesMut};
use std::io;
use tokio_util::codec::Decoder;

pub const ENCRYPTED_BLOCK_SIZE: usize = 65535;
pub const NONCE_SIZE: usize = 12; // ChaCha20 IETF Nonce size
pub const MAC_SIZE: usize = 16;

pub const DATA_BLOCK_SIZE: usize = NONCE_SIZE + ENCRYPTED_BLOCK_SIZE + MAC_SIZE;

pub struct BlockCodec;

impl Decoder for BlockCodec {
  type Item = Bytes;
  type Error = io::Error;

  fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
    // TODO: Check for header and lengths/metadata in it.

    // We don't have enough data, keep reading
    if src.len() < DATA_BLOCK_SIZE {
      src.reserve(DATA_BLOCK_SIZE);
      return Ok(None);
    }

    // Enough data, or more than enough.
    // // let block_size = {
    // //     let mut header = &src[..CRYPT4GH_BLOCK_SIZE];
    // //     header.advance(16);
    // //     usize::from(header.get_u16_le()) + 1
    // // };

    // if src.len() < block_size {
    //     src.reserve(block_size);
    //     return Ok(None);
    // }

    Ok(Some(src.split_to(DATA_BLOCK_SIZE).freeze()))
  }
}
