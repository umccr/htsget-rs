use crate::decrypter::DecrypterStream;
use crate::decrypter::SeekState::NotSeeking;
use crate::error::Result;
use crate::SenderPublicKey;
use crypt4gh::Keys;
use tokio::io::{AsyncRead, AsyncSeek};
use tokio_util::codec::FramedRead;

/// An decrypter reader builder.
#[derive(Debug, Default)]
pub struct Builder {
  sender_pubkey: Option<SenderPublicKey>,
}

impl Builder {
  /// Sets the sender public key
  pub fn with_sender_pubkey(self, sender_pubkey: SenderPublicKey) -> Self {
    self.set_sender_pubkey(Some(sender_pubkey))
  }

  /// Sets the sender public key
  pub fn set_sender_pubkey(mut self, sender_pubkey: Option<SenderPublicKey>) -> Self {
    self.sender_pubkey = sender_pubkey;
    self
  }

  /// Build the decrypter.
  pub fn build<R>(self, inner: R, keys: Vec<Keys>) -> DecrypterStream<R>
  where
    R: AsyncRead,
  {
    DecrypterStream {
      inner: FramedRead::new(inner, Default::default()),
      header_packet_future: None,
      keys,
      sender_pubkey: self.sender_pubkey,
      session_keys: vec![],
      edit_list_packet: None,
      header_length: None,
      current_block_size: None,
      seek_state: NotSeeking,
      stream_length: None,
    }
  }

  /// Build the decrypter and compute the stream length for seek operations. This function will
  /// ensure that recompute_stream_length is called at least once on the decrypter stream.
  ///
  /// This means that data block positions past the end of the stream will be valid and will equal
  /// the the length of the stream. Use the build function if this behaviour is not desired. Seeking
  /// past the end of the stream without a stream length is allowed but the behaviour is dependent
  /// on the underlying reader and data block positions may not be valid.
  pub async fn build_with_stream_length<R>(
    self,
    inner: R,
    keys: Vec<Keys>,
  ) -> Result<DecrypterStream<R>>
  where
    R: AsyncRead + AsyncSeek + Unpin,
  {
    let mut stream = self.build(inner, keys);

    stream.recompute_stream_length().await?;

    Ok(stream)
  }
}
