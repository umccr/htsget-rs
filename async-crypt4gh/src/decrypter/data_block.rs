use std::future::Future;
use std::io::Cursor;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use crypt4gh::{body_decrypt, body_decrypt_parts, WriteInfo};
use pin_project_lite::pin_project;
use tokio::task::JoinHandle;

use crate::error::Error::{Crypt4GHError, JoinHandleError};
use crate::error::Result;
use crate::{DecryptedBytes, DecryptedDataBlock};

pin_project! {
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct DataBlockDecrypter {
        #[pin]
        handle: JoinHandle<Result<DecryptedDataBlock>>
    }
}

impl DataBlockDecrypter {
  pub fn new(
    data_block: Bytes,
    session_keys: Vec<Vec<u8>>,
    edit_list_packet: Option<Vec<u64>>,
  ) -> Self {
    Self {
      handle: tokio::task::spawn_blocking(move || {
        DataBlockDecrypter::decrypt(data_block, session_keys, edit_list_packet)
      }),
    }
  }

  pub fn decrypt(
    data_block: Bytes,
    session_keys: Vec<Vec<u8>>,
    edit_list_packet: Option<Vec<u64>>,
  ) -> Result<DecryptedDataBlock> {
    let size = data_block.len();

    let read_buf = Cursor::new(data_block.to_vec());
    let mut write_buf = Cursor::new(vec![]);
    let mut write_info = WriteInfo::new(0, None, &mut write_buf);

    match edit_list_packet {
      None => body_decrypt(read_buf, session_keys.as_slice(), &mut write_info, 0),
      Some(edit_list) => body_decrypt_parts(read_buf, session_keys, write_info, edit_list),
    }
    .map_err(|err| Crypt4GHError(err.to_string()))?;

    Ok(DecryptedDataBlock::new(
      DecryptedBytes::new(write_buf.into_inner().into()),
      size,
    ))
  }
}

impl Future for DataBlockDecrypter {
  type Output = Result<DecryptedDataBlock>;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    self.project().handle.poll(cx).map_err(JoinHandleError)?
  }
}

#[cfg(test)]
mod tests {
  use crate::decoder::tests::{assert_first_data_block, get_data_block};

  use super::*;

  #[tokio::test]
  async fn data_block_decrypter() {
    let (header_packets, data_block) = get_data_block(0).await;

    let data = DataBlockDecrypter::new(
      data_block,
      header_packets.data_enc_packets,
      header_packets.edit_list_packet,
    )
    .await
    .unwrap();

    assert_first_data_block(data.bytes.to_vec()).await;
  }
}
