use crypt4gh::Keys;
use tokio::io::{AsyncRead, AsyncSeek};
use tokio_util::codec::FramedRead;

use crate::decrypter::DecrypterStream;
use crate::error::Result;
use crate::PublicKey;

/// An decrypter reader builder.
#[derive(Debug, Default)]
pub struct Builder {
  sender_pubkey: Option<PublicKey>,
  stream_length: Option<u64>,
  edit_list: Option<Vec<u64>>,
}

impl Builder {
  /// Sets the sender public key
  pub fn with_sender_pubkey(self, sender_pubkey: PublicKey) -> Self {
    self.set_sender_pubkey(Some(sender_pubkey))
  }

  /// Sets the sender public key
  pub fn set_sender_pubkey(mut self, sender_pubkey: Option<PublicKey>) -> Self {
    self.sender_pubkey = sender_pubkey;
    self
  }

  /// Sets the stream length.
  pub fn with_stream_length(self, stream_length: u64) -> Self {
    self.set_stream_length(Some(stream_length))
  }

  /// Sets the stream length.
  pub fn set_stream_length(mut self, stream_length: Option<u64>) -> Self {
    self.stream_length = stream_length;
    self
  }

  /// Set the edit list manually.
  pub fn with_edit_list(self, edit_list: Vec<u64>) -> Self {
    self.set_edit_list(Some(edit_list))
  }

  /// Set the edit list manually.
  pub fn set_edit_list(mut self, edit_list: Option<Vec<u64>>) -> Self {
    self.edit_list = edit_list;
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
      encrypted_header_packets: None,
      edit_list_packet: DecrypterStream::<()>::create_internal_edit_list(self.edit_list),
      header_info: None,
      header_length: None,
      current_block_size: None,
      stream_length: self.stream_length,
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
    let stream_length = self.stream_length;
    let mut stream = self.build(inner, keys);

    if stream_length.is_none() {
      stream.recompute_stream_length().await?;
    }

    Ok(stream)
  }
}
