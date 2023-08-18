//! Config related to Crypt4GH keys.

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use async_crypt4gh::SenderPublicKey;
use crypt4gh::keys::{get_private_key, get_public_key};
use serde::Deserialize;
use std::path::PathBuf;

/// Config for Crypt4GH keys.
#[derive(Deserialize, Debug, Clone)]
#[serde(try_from = "Crypt4GHPath")]
pub struct Crypt4GHConfig {
  private_key: Vec<u8>,
  sender_public_key: Option<SenderPublicKey>,
}

impl Crypt4GHConfig {
  /// Create a new Crypt4GH config.
  pub fn new(private_key: Vec<u8>, sender_public_key: Option<SenderPublicKey>) -> Self {
    Self {
      private_key,
      sender_public_key,
    }
  }

  /// Get the private key used to decrypt the data.
  pub fn private_key(&self) -> &Vec<u8> {
    &self.private_key
  }

  /// Get the sender key to verify the encrypted data.
  pub fn sender_public_key(&self) -> &Option<SenderPublicKey> {
    &self.sender_public_key
  }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Crypt4GHPath {
  private_key: PathBuf,
  sender_public_key: Option<PathBuf>,
}

impl TryFrom<Crypt4GHPath> for Crypt4GHConfig {
  type Error = Error;

  fn try_from(crypt4gh_path: Crypt4GHPath) -> Result<Self> {
    let private_key = get_private_key(&crypt4gh_path.private_key, || Ok("".to_string()))
      .map_err(|err| ParseError(format!("loading private key: {}", err)))?;

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
