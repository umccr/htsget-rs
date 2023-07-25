use crate::storage::crypt4gh::error::Error::{Crypt4GHError, JoinHandleError};
use crate::storage::crypt4gh::error::Result;
use crate::storage::crypt4gh::PlainTextBytes;
use bytes::Bytes;
use crypt4gh::{body_decrypt, WriteInfo};
use futures::Stream;
use pin_project_lite::pin_project;
use std::future::Future;
use std::io::Cursor;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::task::JoinHandle;

pin_project! {
    pub struct DataBlockDecryptor {
        #[pin]
        handle: JoinHandle<Result<PlainTextBytes>>
    }
}

impl DataBlockDecryptor {
  pub fn new(data_block: Bytes, keys: Vec<Vec<u8>>) -> Self {
    Self {
      handle: tokio::task::spawn_blocking(move || DataBlockDecryptor::decrypt(data_block, keys)),
    }
  }

  pub fn decrypt(data_block: Bytes, keys: Vec<Vec<u8>>) -> Result<PlainTextBytes> {
    let read_buf = Cursor::new(data_block.to_vec());
    let mut write_buf = Cursor::new(vec![]);
    // Todo allow limit to be passed here.
    let mut write_info = WriteInfo::new(0, None, &mut write_buf);

    body_decrypt(read_buf, keys.as_slice(), &mut write_info, 0).map_err(Crypt4GHError)?;

    Ok(PlainTextBytes(write_buf.into_inner().into()))
  }
}

impl Future for DataBlockDecryptor {
  type Output = Result<PlainTextBytes>;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    self.project().handle.poll(cx).map_err(JoinHandleError)?
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::storage::crypt4gh::decoder::tests::{assert_first_data_block, get_first_data_block};

  #[tokio::test]
  async fn data_block_decryptor() {
    let (header_packets, data_block) = get_first_data_block().await;

    let data = DataBlockDecryptor::new(data_block, header_packets.data_enc_packets)
      .await
      .unwrap();

    assert_first_data_block(data.0.to_vec()).await;
  }
}
