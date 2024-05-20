//! Config related to Crypt4GH keys.

use std::path::PathBuf;

use crypt4gh::keys::{get_private_key, get_public_key};
use serde::{Deserialize, Serialize};
use tracing::warn;

use async_crypt4gh::{KeyPair, PublicKey};

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crate::tls::load_key;

/// Wrapper around a private key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "PathBuf", into = "Vec<u8>")]
pub struct PrivateKey(rustls::PrivateKey);

impl PrivateKey {
  /// Get the inner value.
  pub fn into_inner(self) -> rustls::PrivateKey {
    self.0
  }
}

impl AsRef<rustls::PrivateKey> for PrivateKey {
  fn as_ref(&self) -> &rustls::PrivateKey {
    &self.0
  }
}

impl TryFrom<PathBuf> for PrivateKey {
  type Error = Error;

  fn try_from(path: PathBuf) -> Result<Self> {
    Ok(PrivateKey(load_key(path)?))
  }
}

impl From<PrivateKey> for Vec<u8> {
  fn from(key: PrivateKey) -> Self {
    key.into_inner().0
  }
}

/// Config for Crypt4GH keys.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "Crypt4GHPath")]
pub struct Crypt4GHKeyPair {
  key_pair: KeyPair,
}

impl Crypt4GHKeyPair {
  /// Create a new Crypt4GH config.
  pub fn new(key_pair: KeyPair) -> Self {
    Self { key_pair }
  }

  /// Get the key pair
  pub fn key_pair(&self) -> &KeyPair {
    &self.key_pair
  }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Crypt4GHPath {
  private_key: PathBuf,
  public_key: PathBuf,
}

impl TryFrom<Crypt4GHPath> for Crypt4GHKeyPair {
  type Error = Error;

  fn try_from(crypt4gh_path: Crypt4GHPath) -> Result<Self> {
    let private_key = get_private_key(crypt4gh_path.private_key.clone(), Ok("".to_string()));

    let private_key = match private_key {
      Ok(key) => key,
      Err(err) => {
        warn!(
          err = err.to_string(),
          "error getting crypt4gh key, falling back to rustls key"
        );
        PrivateKey::try_from(crypt4gh_path.private_key)
          .map_err(|_| ParseError(format!("failed to parse crypt4gh key: {}", err)))?
          .into_inner()
          .0
      }
    };

    let parse_public_key = |key: Option<PathBuf>| {
      Ok(
        key
          .map(|key| {
            get_public_key(key).map_err(|err| ParseError(format!("loading public key: {}", err)))
          })
          .transpose()?
          .map(PublicKey::new),
      )
    };

    Ok(Self::new(KeyPair::new(
      rustls::PrivateKey(private_key),
      parse_public_key(Some(crypt4gh_path.public_key))?.expect("expected valid public key"),
    )))
  }
}
