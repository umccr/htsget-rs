//! Crypt4GH key parsing.
//!

use crate::error::Error::{IoError, ParseError};
use crate::error::{Error, Result};
use crate::storage::c4gh::local::C4GHLocal;
#[cfg(feature = "s3-storage")]
use crate::storage::c4gh::secrets_manager::C4GHSecretsManager;
use crypt4gh::error::Crypt4GHError;
use futures_util::future::{BoxFuture, Shared};
use futures_util::FutureExt;
use serde::Deserialize;
use tokio::task::{JoinError, JoinHandle};

pub mod local;

#[cfg(feature = "s3-storage")]
pub mod secrets_manager;

/// Config for Crypt4GH keys.
#[derive(Deserialize, Debug, Clone)]
#[serde(try_from = "Location")]
pub struct C4GHKeys {
  // Store a cloneable future so that it can be resolved outside serde.
  keys: Shared<BoxFuture<'static, Result<Vec<crypt4gh::Keys>>>>,
}

impl C4GHKeys {
  /// Get the inner value.
  pub async fn keys(self) -> Result<Vec<crypt4gh::Keys>> {
    self.keys.await
  }

  /// Construct the C4GH keys from a key pair.
  pub fn from_key_pair(private_key: Vec<u8>, recipient_public_key: Vec<u8>) -> Vec<crypt4gh::Keys> {
    vec![crypt4gh::Keys {
      method: 0,
      privkey: private_key,
      recipient_pubkey: recipient_public_key,
    }]
  }

  pub fn from_join_handle(handle: JoinHandle<Result<Vec<crypt4gh::Keys>>>) -> Self {
    Self {
      keys: handle.map(|value| value?).boxed().shared(),
    }
  }
}

impl From<JoinError> for Error {
  fn from(err: JoinError) -> Self {
    IoError(err.to_string())
  }
}

impl From<Crypt4GHError> for Error {
  fn from(err: Crypt4GHError) -> Self {
    ParseError(err.to_string())
  }
}

impl TryFrom<Location> for C4GHKeys {
  type Error = Error;

  fn try_from(location: Location) -> Result<Self> {
    match location {
      Location::Local(local) => local.try_into(),
      #[cfg(feature = "s3-storage")]
      Location::SecretsManager(secrets_manager) => secrets_manager.try_into(),
    }
  }
}

/// The location of C4GH keys.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "location", deny_unknown_fields)]
#[non_exhaustive]
pub enum Location {
  #[serde(alias = "local", alias = "LOCAL")]
  Local(C4GHLocal),
  #[cfg(feature = "s3-storage")]
  #[serde(alias = "secretsmanager", alias = "SECRETSMANAGER")]
  SecretsManager(C4GHSecretsManager),
}