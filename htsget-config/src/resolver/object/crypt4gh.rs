use crate::tls::PrivateKey;
use serde::{Deserialize, Serialize};

/// Object type configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Crypt4GHObject {
  decryption_key: PrivateKey,
}

impl Crypt4GHObject {
  /// Get the private decryption key.
  pub fn key(&self) -> &rustls::PrivateKey {
    self.decryption_key.as_ref()
  }

  /// Get the owned root store pair.
  pub fn into_inner(self) -> rustls::PrivateKey {
    self.decryption_key.into_inner()
  }
}
