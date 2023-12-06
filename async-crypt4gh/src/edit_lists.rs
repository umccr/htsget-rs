use crate::PublicKey;
use rustls::PrivateKey;

/// Unencrypted byte range positions.
#[derive(Debug, Clone)]
pub struct UnencryptedRange {
  start: u64,
  end: u64,
}

impl UnencryptedRange {
  pub fn new(start: u64, end: u64) -> Self {
    Self { start, end }
  }

  pub fn start(&self) -> u64 {
    self.start
  }

  pub fn end(&self) -> u64 {
    self.end
  }
}

/// Add edit lists to the header packet. Returns `None` if an edit list already exists.
pub fn add_edit_lists(
  header: Vec<u8>,
  unencrypted_ranges: Vec<UnencryptedRange>,
  private_key: PrivateKey,
  recipient_public_key: PublicKey,
) -> Option<Vec<u8>> {
  todo!();
}
