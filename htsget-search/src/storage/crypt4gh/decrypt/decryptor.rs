use crate::storage::crypt4gh::decrypt::decoder::{
    Block, DecodedBlock, ENCRYPTED_BLOCK_SIZE, MAC_SIZE, NONCE_SIZE,
};
use crate::storage::crypt4gh::error::Error::JoinHandleError;
use bytes::Bytes;
use crypt4gh::Keys;
use crate::storage::crypt4gh::error::Result;
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

#[derive(Debug, Clone)]
pub struct SenderPublicKey {
    bytes: Vec<u8>,
}

// Decrypts/encrypts one block?
pin_project! {
    struct Decryptor<R> {
        #[pin]
        inner: FramedRead<R, Block>,
        keys: Vec<Keys>,
        sender_pubkey: Option<SenderPublicKey>
    }
}

impl<R> Decryptor<R>
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

impl<R> Stream for Decryptor<R>
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
                    DecodedBlock::HeaderInfo(_) => {
                        // Nothing to do on header info.
                        Poll::Pending
                    }
                    DecodedBlock::HeaderPacket(_) => {
                        todo!()
                    }
                    DecodedBlock::DataBlock(buf) => {
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

#[derive(Debug)]
pub struct PlainTextBytes(Bytes);

pin_project! {
    pub struct DataBlockDecryptor {
        #[pin]
        handle: JoinHandle<Result<PlainTextBytes>>
    }
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