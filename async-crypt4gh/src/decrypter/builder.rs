use crate::decrypter::DecrypterStream;
use crate::decrypter::SeekState::NotSeeking;
use crate::SenderPublicKey;
use crypt4gh::Keys;
use tokio::io::AsyncRead;
use tokio_util::codec::FramedRead;

/// An decrypter reader builder.
#[derive(Debug, Default)]
pub struct Builder {
  length_hint: Option<u64>,
  sender_pubkey: Option<SenderPublicKey>,
}

impl Builder {
  /// Sets the length hint.
  pub fn with_length_hint(self, length_hint: u64) -> Self {
    self.set_length_hint(Some(length_hint))
  }

  /// Sets the sender public key
  pub fn with_sender_pubkey(self, sender_pubkey: SenderPublicKey) -> Self {
    self.set_sender_pubkey(Some(sender_pubkey))
  }

  /// Sets the length hint.
  pub fn set_length_hint(mut self, length_hint: Option<u64>) -> Self {
    self.length_hint = length_hint;
    self
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
      length_hint: self.length_hint,
    }
  }
}
