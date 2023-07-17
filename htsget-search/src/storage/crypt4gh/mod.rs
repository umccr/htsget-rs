use crate::storage::crypt4gh::block::{
  Block, BlockType, ENCRYPTED_BLOCK_SIZE, MAC_SIZE, NONCE_SIZE,
};
use crate::storage::crypt4gh::error::Error::JoinHandleError;
use bytes::Bytes;
use crypt4gh::Keys;
use error::Result;
use futures::ready;
use futures::Stream;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{cmp, io};
use tokio::io::AsyncBufRead;
use tokio::io::AsyncRead;
use tokio::task::JoinHandle;
use tokio_util::codec::FramedRead;

pub mod block;
pub mod error;

#[derive(Debug, Clone)]
pub struct SenderPublicKey {
  bytes: Vec<u8>,
}

// #[async_trait]
// pub trait Crypt4gh {
//     type Streamable: AsyncRead + Unpin + Send + Sync;

//     /// Decrypts the header of the underlying file.
//     async fn decrypt_header(&self, encrypted_data: Self::Streamable, private_keys: Keys, sender_public_key: SenderPublicKey) -> Self::Streamable {
//         let mut chunk: [u8; 65535];
//         encrypted_data.read_exact(&mut chunk).await.unwrap();

//         crypt4gh::decrypt(keys, read_buffer, write_buffer, range_start, range_span, sender_pubkey)

//         panic!();
//     }
// }

pin_project! {
    pub struct Crypt4gh<R> {
        #[pin]
        inner: R,
        keys: Keys,
        sender_pubkey: Option<SenderPublicKey>
    }
}

impl<R> Crypt4gh<R> {
  pub fn new(inner: R, keys: Keys, sender_pubkey: Option<SenderPublicKey>) -> Self {
    Self {
      inner,
      keys,
      sender_pubkey,
    }
  }
}

impl<R> AsyncRead for Crypt4gh<R>
where
  R: AsyncRead,
{
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut tokio::io::ReadBuf<'_>,
  ) -> Poll<io::Result<()>> {
    let src = ready!(self.as_mut().poll_fill_buf(cx))?;

    let amt = cmp::min(src.len(), buf.remaining());
    buf.put_slice(&src[..amt]);

    self.consume(amt);

    Poll::Ready(Ok(()))

    // let this = self.project();

    // // TODO: read the number of bytes we need, e.g. 64kb per block
    // // TODO: loop over the whole async read.
    // match ready!(this.inner.read_exact(cx)) {
    //     Some(Ok(buf)) => Poll::Ready(Ok(Cryptor::new(buf, this.keys, this.sender_pubkey).decrypt())),
    //     Some(Err(e)) => Poll::Ready(Err(e)),
    //     None => Poll::Ready(None),
    // }
  }

  // fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
  //     match self.project().inner.poll_next(cx) {

  //     }
  //     match ready!(self.project().inner.poll_next(cx)) {
  //         Some(Ok(buf)) => Poll::Ready(Some(Ok(Inflate::new(buf)))),
  //         Some(Err(e)) => Poll::Ready(Some(Err(e))),
  //         None => Poll::Ready(None),
  //     }
  // }
}

impl<R> AsyncBufRead for Crypt4gh<R>
where
  R: AsyncRead,
{
  fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
    let this = self.project();

    todo!();
    // if !this.block.data().has_remaining() {
    //   let mut stream = this.stream.as_pin_mut().expect("missing stream");
    //
    //   loop {
    //     match ready!(stream.as_mut().poll_next(cx)) {
    //       Some(Ok(mut block)) => {
    //         block.set_position(*this.position);
    //         *this.position += block.size();
    //         let data_len = block.data().len();
    //         *this.block = block;
    //
    //         if data_len > 0 {
    //           break;
    //         }
    //       }
    //       Some(Err(e)) => return Poll::Ready(Err(e)),
    //       None => return Poll::Ready(Ok(&[])),
    //     }
    //   }
    // }
    //
    // return Poll::Ready(Ok(this.block.data().as_ref()));
  }

  fn consume(self: Pin<&mut Self>, _amt: usize) {}
}

// Decrypts/encrypts one block?
pin_project! {
    struct DataBlockStreamDecryptor<R> {
        #[pin]
        inner: FramedRead<R, Block>,
        keys: Vec<Keys>,
        sender_pubkey: Option<SenderPublicKey>
    }
}

impl<R> DataBlockStreamDecryptor<R>
where
  R: AsyncRead,
{
  pub fn new(inner: R, keys: Vec<Keys>, sender_pubkey: Option<SenderPublicKey>) -> Self {
    Self {
      inner: FramedRead::new(inner, Default::default()),
      keys,
      sender_pubkey,
    }
  }
}

impl<R> Stream for DataBlockStreamDecryptor<R>
where
  R: AsyncRead,
{
  type Item = Result<DataBlockDecryptor>;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let this = self.project();

    // (Nonce + 64kb + MAC) block from decoder.
    // This is an encryted data block.
    let item = this.inner.poll_next(cx);

    match ready!(item) {
      Some(Ok(buf)) => {
        match buf {
          BlockType::HeaderInfo(_) => {
            todo!()
          }
          BlockType::HeaderPacket(_) => {
            todo!()
          }
          BlockType::DataBlock(buf) => {
            Poll::Ready(Some(Ok(DataBlockDecryptor::new(
              buf,
              // Todo make this so it doesn't use owned Keys and SenderPublicKey as it will be called asynchronously.
              this.keys.clone(),
              this.sender_pubkey.clone(),
            ))))
          }
        }
      }
      Some(Err(e)) => Poll::Ready(Some(Err(e))),
      None => Poll::Ready(None),
    }
  }
}

pin_project! {
    pub struct DataBlockDecryptor {
        #[pin]
        handle: JoinHandle<Result<PlainTextBytes>>
    }
}

#[derive(Debug)]
pub struct PlainTextBytes(Bytes);

#[derive(Debug)]
pub struct DataBlock {
  nonce: [u8; NONCE_SIZE],
  encrypted_data: [u8; ENCRYPTED_BLOCK_SIZE],
  mac: [u8; MAC_SIZE],
}

impl DataBlockDecryptor {
  fn new(src: Bytes, keys: Vec<Keys>, sender_pubkey: Option<SenderPublicKey>) -> Self {
    Self {
      handle: tokio::task::spawn_blocking(move || {
        DataBlockDecryptor::decrypt(src, keys, sender_pubkey)
      }),
    }
  }

  fn decrypt(
    src: Bytes,
    keys: Vec<Keys>,
    sender_pubkey: Option<SenderPublicKey>,
  ) -> Result<PlainTextBytes> {
    let mut read_buffer = io::Cursor::new(src);
    let mut write_buffer = io::Cursor::new(vec![]);

    crypt4gh::decrypt(
      keys.as_slice(),
      &mut read_buffer,
      &mut write_buffer,
      0,
      None,
      &sender_pubkey.map(|pubkey| pubkey.bytes),
    )
    .map_err(|err| {
      io::Error::new(
        io::ErrorKind::Other,
        format!("decrypting read buffer: {}", err),
      )
    })?;

    Ok(PlainTextBytes(write_buffer.into_inner().into()))
  }
}

impl Future for DataBlockDecryptor {
  type Output = Result<PlainTextBytes>;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    self.project().handle.poll(cx).map_err(JoinHandleError)?
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::io;

  use super::*;
  use hex_literal::hex;

  const PLAINTEXT: &[u8] = &[
    0xbe, 0x07, 0x5f, 0xc5, 0x3c, 0x81, 0xf2, 0xd5, 0xcf, 0x14, 0x13, 0x16, 0xeb, 0xeb, 0x0c, 0x7b,
    0x52, 0x28, 0xc5, 0x2a, 0x4c, 0x62, 0xcb, 0xd4, 0x4b, 0x66, 0x84, 0x9b, 0x64, 0x24, 0x4f, 0xfc,
    0xe5, 0xec, 0xba, 0xaf, 0x33, 0xbd, 0x75, 0x1a, 0x1a, 0xc7, 0x28, 0xd4, 0x5e, 0x6c, 0x61, 0x29,
    0x6c, 0xdc, 0x3c, 0x01, 0x23, 0x35, 0x61, 0xf4, 0x1d, 0xb6, 0x6c, 0xce, 0x31, 0x4a, 0xdb, 0x31,
    0x0e, 0x3b, 0xe8, 0x25, 0x0c, 0x46, 0xf0, 0x6d, 0xce, 0xea, 0x3a, 0x7f, 0xa1, 0x34, 0x80, 0x57,
    0xe2, 0xf6, 0x55, 0x6a, 0xd6, 0xb1, 0x31, 0x8a, 0x02, 0x4a, 0x83, 0x8f, 0x21, 0xaf, 0x1f, 0xde,
    0x04, 0x89, 0x77, 0xeb, 0x48, 0xf5, 0x9f, 0xfd, 0x49, 0x24, 0xca, 0x1c, 0x60, 0x90, 0x2e, 0x52,
    0xf0, 0xa0, 0x89, 0xbc, 0x76, 0x89, 0x70, 0x40, 0xe0, 0x82, 0xf9, 0x37, 0x76, 0x38, 0x48, 0x64,
    0x5e, 0x07, 0x05,
  ];

  const CIPHERTEXT: &[u8] = &[
    0x0c, 0x9a, 0x1b, 0xfa, 0x07, 0x05, 0x85, 0xae, 0xb8, 0xcb, 0xdd, 0x80, 0xf3, 0x5d, 0xb1, 0x55,
    0x8b, 0x14, 0xa9, 0xa2, 0x11, 0xc5, 0x28, 0x18, 0xc0, 0x78, 0x69, 0x90, 0xda, 0x61, 0x84, 0x63,
    0xdb, 0x80, 0x9d, 0x3a, 0x93, 0x94, 0x76, 0x48, 0xd1, 0x4b, 0x9f, 0xa9, 0x17, 0x9a, 0xf7, 0x8f,
    0x20, 0x33, 0xef, 0x0f, 0x2a, 0xe5, 0x8a, 0xcf, 0x7f, 0x4b, 0x3d, 0x5e, 0x8e, 0x05, 0x9e, 0x96,
    0x31, 0xe3, 0xc8, 0x86, 0x7d, 0x94, 0x3e, 0x90, 0x79, 0xfa, 0x88, 0x87, 0xed, 0x01, 0x3c, 0xb6,
    0xba, 0x0a, 0x1a, 0xed, 0xcb, 0x79, 0x5c, 0x65, 0x6b, 0xfa, 0xe5, 0xb7, 0xe4, 0xf8, 0x65, 0x60,
    0x5d, 0xe3, 0x93, 0x12, 0x4b, 0x63, 0x18, 0xe2, 0x61, 0xc3, 0x94, 0x88, 0xf3, 0x46, 0xfc, 0xa9,
    0xf9, 0xe1, 0x9d, 0x34, 0xb3, 0xaa, 0xb0, 0x56, 0x44, 0x3c, 0xa5, 0xdc, 0xe2, 0x9a, 0xf1, 0xba,
    0xf5, 0xaf, 0xd2, 0x16, 0x34, 0x36, 0xd8, 0x65, 0xc7, 0x34, 0xc5, 0x79, 0x4c, 0x4e, 0x7e, 0xbe,
    0x88, 0xe3, 0xdf,
  ];

  // Alice's keypair
  const ALICE_SECRET_KEY: [u8; 32] =
    hex!("68f208412d8dd5db9d0c6d18512e86f0ec75665ab841372d57b042b27ef89d4c");
  const ALICE_PUBLIC_KEY: [u8; 32] =
    hex!("ac3a70ba35df3c3fae427a7c72021d68f2c1e044040b75f17313c0c8b5d4241d");

  // Bob's keypair
  const BOB_SECRET_KEY: [u8; 32] =
    hex!("b581fb5ae182a16f603f39270d4e3b95bc008310b727a11dd4e784a0044d461b");
  const BOB_PUBLIC_KEY: [u8; 32] =
    hex!("e8980c86e032f1eb2975052e8d65bddd15c3b59641174ec9678a53789d92c754");

  // TODO: Write tests for different levels of Decrytor/DataBlocks/etc...

  #[tokio::test]
  async fn crypt4gh_encrypt() {
    let keys = Keys {
      method: 0,
      privkey: Vec::from(ALICE_SECRET_KEY),
      recipient_pubkey: Vec::from(BOB_PUBLIC_KEY),
    };
    // let ciphertext = DataBlockStreamDecryptor::new(
    //   PLAINTEXT,
    //   vec![keys],
    //   Some(SenderPublicKey {
    //     bytes: Vec::from(BOB_PUBLIC_KEY),
    //   }),
    // )
    // .encrypt();
    //
    // assert_eq!(CIPHERTEXT, ciphertext);
  }

  #[tokio::test]
  async fn crypt4gh_decrypt() -> io::Result<()> {
    todo!()
    // let data_blocks = ...;

    // let reader = BgzfReader::new(Crypt4ghReader::new(data_blocks));

    // reader.read_block();

    // let plaintext = DataBlockStreamDecryptor::new().decrypt();
    //
    // assert_eq!(PLAINTEXT, plaintext);
  }
}
