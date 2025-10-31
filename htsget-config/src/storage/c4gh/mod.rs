//! Crypt4GH key parsing.
//!

use crate::error::Error::{IoError, ParseError};
use crate::error::{Error, Result};
use crate::storage::c4gh::header::C4GHHeader;
use crate::storage::c4gh::local::C4GHLocal;
#[cfg(feature = "aws")]
use crate::storage::c4gh::secrets_manager::C4GHSecretsManager;
use crypt4gh::error::Crypt4GHError;
use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared};
use schemars::JsonSchema;
use serde::Deserialize;
use std::pin::Pin;
use tokio::task::{JoinError, JoinHandle};

pub mod local;

mod header;
#[cfg(feature = "aws")]
pub mod secrets_manager;

/// Specifies the location of a Crypt4GH key.
#[derive(JsonSchema, Deserialize, Debug, Clone)]
#[serde(try_from = "C4GHKeyLocation", deny_unknown_fields)]
pub struct C4GHKeys {
  #[schemars(with = "C4GHKeyLocation")]
  // Store a cloneable future so that it can be resolved outside serde.
  keys: Shared<BoxFuture<'static, Result<Vec<crypt4gh::Keys>>>>,
  using_header: Option<C4GHHeader>,
}

impl C4GHKeys {
  /// Get the inner values.
  pub async fn into_inner(self) -> Result<(Vec<crypt4gh::Keys>, Option<C4GHHeader>)> {
    Ok((self.keys.await?, self.using_header))
  }

  /// Construct the C4GH keys from a key pair.
  pub fn from_key_pair(private_key: Vec<u8>, recipient_public_key: Vec<u8>) -> Vec<crypt4gh::Keys> {
    vec![crypt4gh::Keys {
      method: 0,
      privkey: private_key,
      recipient_pubkey: recipient_public_key,
    }]
  }

  /// Construct from an existing join handle.
  pub fn from_join_handle(
    handle: JoinHandle<Result<Vec<crypt4gh::Keys>>>,
    using_header: Option<C4GHHeader>,
  ) -> Self {
    Self {
      keys: handle.map(|value| value?).boxed().shared(),
      using_header,
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

/// Specifies the location of a Crypt4GH key.
#[derive(JsonSchema, Deserialize, Debug, Clone)]
#[serde(tag = "kind", deny_unknown_fields)]
#[non_exhaustive]
pub enum C4GHKeyType {
  /// Obtain keys from a local file.
  #[serde(alias = "file", alias = "FILE")]
  File(C4GHLocal),
  /// Obtain keys from AWS secrets manager.
  #[cfg(feature = "aws")]
  #[serde(alias = "secretsmanager", alias = "SECRETSMANAGER")]
  SecretsManager(C4GHSecretsManager),
  /// Obtain keys from a header that comes with the request.
  #[serde(alias = "header", alias = "HEADER")]
  Header(C4GHHeader),
}

impl C4GHKeyType {
  /// Create a key type from a local file.
  pub fn new_file(file: C4GHLocal) -> Self {
    Self::File(file)
  }

  /// Create a key type from AWS secrets manager.
  #[cfg(feature = "aws")]
  pub fn new_secrets_manager(secrets_manager: C4GHSecretsManager) -> Self {
    Self::SecretsManager(secrets_manager)
  }
}

#[derive(JsonSchema, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct C4GHKeyLocation {
  private: C4GHKeyType,
  public: C4GHKeyType,
}

impl C4GHKeyLocation {
  /// Create a new C4GH location.
  pub fn new(private: C4GHKeyType, public: C4GHKeyType) -> Self {
    Self { private, public }
  }
}

impl TryFrom<C4GHKeyLocation> for C4GHKeys {
  type Error = Error;

  fn try_from(location: C4GHKeyLocation) -> Result<Self> {
    let private_key: Pin<Box<dyn Future<Output = _> + Send>> = match location.private {
      C4GHKeyType::File(file) => Box::pin(async { file.into_private_key() }),
      #[cfg(feature = "aws")]
      C4GHKeyType::SecretsManager(secrets_manager) => Box::pin(secrets_manager.into_private_key()),
      C4GHKeyType::Header(_) => {
        return Err(ParseError(
          "using a header for private keys is unsupported".to_string(),
        ));
      }
    };
    let (recipient_public, using_header): (Pin<Box<dyn Future<Output = _> + Send>>, _) =
      match location.public {
        C4GHKeyType::File(file) => (Box::pin(async { file.into_public_key() }), None),
        #[cfg(feature = "aws")]
        C4GHKeyType::SecretsManager(secrets_manager) => {
          (Box::pin(secrets_manager.into_public_key()), None)
        }
        C4GHKeyType::Header(using_header) => (Box::pin(async { Ok(vec![]) }), Some(using_header)),
      };

    Ok(C4GHKeys::from_join_handle(
      tokio::spawn(async move {
        let private_key = private_key.await?;
        let recipient_public = recipient_public.await?;

        Ok(C4GHKeys::from_key_pair(private_key, recipient_public))
      }),
      using_header,
    ))
  }
}
