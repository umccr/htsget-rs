//! Config related to Crypt4GH keys.

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crate::tls::PrivateKey;
use async_crypt4gh::SenderPublicKey;
use crypt4gh::keys::{get_private_key, get_public_key};
use serde::Deserialize;
use std::path::PathBuf;
use tracing::warn;

/// Config for Crypt4GH keys.
#[derive(Deserialize, Debug, Clone)]
#[serde(try_from = "Crypt4GHPath")]
pub struct Crypt4GH {
  decryption_key: Vec<u8>,
  sender_public_key: Option<SenderPublicKey>,
}

impl Crypt4GH {
  /// Create a new Crypt4GH config.
  pub fn new(decryption_key: Vec<u8>, sender_public_key: Option<SenderPublicKey>) -> Self {
    Self {
      decryption_key,
      sender_public_key,
    }
  }

  /// Get the private key used to decrypt the data.
  pub fn private_key(&self) -> &Vec<u8> {
    &self.decryption_key
  }

  /// Get the sender key to verify the encrypted data.
  pub fn sender_public_key(&self) -> &Option<SenderPublicKey> {
    &self.sender_public_key
  }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Crypt4GHPath {
  decryption_key: PathBuf,
  sender_public_key: Option<PathBuf>,
}

impl TryFrom<Crypt4GHPath> for Crypt4GH {
  type Error = Error;

  fn try_from(crypt4gh_path: Crypt4GHPath) -> Result<Self> {
    let private_key = get_private_key(&crypt4gh_path.decryption_key, || Ok("".to_string()));

    let private_key = match private_key {
      Ok(key) => key,
      Err(err) => {
        warn!(
          err = err.to_string(),
          "error getting crypt4gh key, falling back to rustls key"
        );
        PrivateKey::try_from(crypt4gh_path.decryption_key)
          .map_err(|_| ParseError(format!("failed to parse crypt4gh key: {}", err)))?
          .into_inner()
          .0
      }
    };

    let sender_public_key = crypt4gh_path
      .sender_public_key
      .map(|key| {
        get_public_key(&key)
          .map_err(|err| ParseError(format!("loading sender public key: {}", err)))
      })
      .transpose()?
      .map(SenderPublicKey::new);

    Ok(Self::new(private_key, sender_public_key))
  }
}
