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

pub mod header;
pub mod local;
#[cfg(feature = "aws")]
pub mod secrets_manager;

/// Specifies the location of a Crypt4GH key.
#[derive(JsonSchema, Deserialize, Debug, Clone)]
#[serde(try_from = "C4GHKeySet", deny_unknown_fields)]
pub struct C4GHKeys {
  // Store a cloneable futures so that they can be resolved outside serde.
  #[schemars(with = "C4GHKeyLocation")]
  server_decryption_keys: Shared<BoxFuture<'static, Result<Vec<crypt4gh::Keys>>>>,
  #[schemars(with = "C4GHKeyLocation")]
  client_encryption_keys: Shared<BoxFuture<'static, Result<Vec<crypt4gh::Keys>>>>,
  client_key_from_header: Option<C4GHHeader>,
}

impl C4GHKeys {
  /// Get the inner values.
  pub async fn into_inner(self) -> Result<(Vec<crypt4gh::Keys>, Vec<crypt4gh::Keys>, Option<C4GHHeader>)> {
    Ok((self.server_decryption_keys.await?, self.client_encryption_keys.await?, self.client_key_from_header))
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
    server_keys: JoinHandle<Result<Vec<crypt4gh::Keys>>>,
    client_keys: JoinHandle<Result<Vec<crypt4gh::Keys>>>,
    client_key_from_header: Option<C4GHHeader>,
  ) -> Self {
    Self {
      server_decryption_keys: server_keys.map(|value| value?).boxed().shared(),
      client_encryption_keys: client_keys.map(|value| value?).boxed().shared(),
      client_key_from_header,
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

  /// Create a key type for obtaining keys from a header.
  pub fn new_header(header: C4GHHeader) -> Self {
    Self::Header(header)
  }
}

/// The specific location for a private and public key pair. If the private key is
/// unspecified then the public key is used for encryption, otherwise decryption is
/// used.
#[derive(JsonSchema, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct C4GHKeyLocation {
  private: Option<C4GHKeyType>,
  public: C4GHKeyType,
}

impl C4GHKeyLocation {
  /// Create a new C4GH location.
  pub fn new(private: Option<C4GHKeyType>, public: C4GHKeyType) -> Self {
    Self { private, public }
  }
}

/// A keyset comprising the server's key pair and the client's public key, which will be used
/// to re-encrypt the header for the client.
#[derive(JsonSchema, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct C4GHKeySet {
  server: C4GHKeyLocation,
  client: C4GHKeyLocation,
}

impl TryFrom<C4GHKeySet> for C4GHKeys {
  type Error = Error;

  fn try_from(location: C4GHKeySet) -> Result<Self> {
    let extract_private_key = |private_key| -> Result<Pin<Box<dyn Future<Output = _> + Send>>> {
      match private_key {
        Some(C4GHKeyType::File(file)) => Ok(Box::pin(async { file.into_private_key() })),
        #[cfg(feature = "aws")]
        Some(C4GHKeyType::SecretsManager(secrets_manager)) => Ok(Box::pin(secrets_manager.into_private_key())),
        Some(C4GHKeyType::Header(_)) => {
          Err(ParseError(
            "using a header for private keys is unsupported".to_string(),
          ))
        }
        _ => {
          Err(ParseError("missing server private key".to_string()))
        }
      }
    };
    let extract_public_key = |public_key| -> (Pin<Box<dyn Future<Output = _> + Send>>, _) {
      match public_key {
        C4GHKeyType::File(file) => (Box::pin(async { file.into_public_key() }), None),
        #[cfg(feature = "aws")]
        C4GHKeyType::SecretsManager(secrets_manager) => {
          (Box::pin(secrets_manager.into_public_key()), None)
        }
        C4GHKeyType::Header(using_header) => (Box::pin(async { Ok(vec![]) }), Some(using_header)),
      }
    };

    let server_decryption_private = extract_private_key(location.server.private.clone())?;
    let server_encryption_private = extract_private_key(location.server.private)?;

    if location.client.private.is_some() {
      return Err(ParseError("the client's private key should not be specified".to_string()));
    }

    let (server_public_key, _) = extract_public_key(location.server.public);
    let (client_public_key, client_key_from_header) = extract_public_key(location.client.public);

    Ok(C4GHKeys::from_join_handle(
      tokio::spawn(async move {
        // Server decrypts using it's own private key and public key.
        Ok(C4GHKeys::from_key_pair(server_decryption_private.await?, server_public_key.await?))
      }),
      tokio::spawn(async move {
        // Server encrypts using it's own private key for the client who is the recipient.
        Ok(C4GHKeys::from_key_pair(server_encryption_private.await?, client_public_key.await?))
      }),
      client_key_from_header,
    ))
  }
}
