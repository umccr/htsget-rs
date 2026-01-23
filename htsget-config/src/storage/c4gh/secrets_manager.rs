//! Obtain C4GH keys from AWS secrets manager.
//!

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use aws_config::{BehaviorVersion, load_defaults};
use aws_sdk_secretsmanager::Client;
use aws_sdk_secretsmanager::error::SdkError;
use crypt4gh::keys::{get_private_key, get_public_key};
use schemars::JsonSchema;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use tempfile::NamedTempFile;

/// Specify keys on AWS secrets manager.
#[derive(JsonSchema, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct C4GHSecretsManager {
  /// The ARN or name of the secret holding the private or public key.
  key: String,
  #[serde(skip)]
  client: Option<Client>,
}

impl C4GHSecretsManager {
  /// Create a new C4GH secrets manager key storage.
  pub fn new(key: String) -> Self {
    Self { key, client: None }
  }

  /// Set the client.
  pub fn with_client(mut self, client: Client) -> Self {
    self.client = Some(client);
    self
  }

  /// Retrieve a binary secret from secrets manager.
  pub async fn get_secret(client: &Client, id: impl Into<String>) -> Result<Vec<u8>> {
    let secret = client.get_secret_value().secret_id(id).send().await?;

    if let Some(secret) = secret.secret_binary {
      Ok(secret.into_inner())
    } else if let Some(secret) = secret.secret_string {
      Ok(secret.into_bytes())
    } else {
      Err(ParseError("failed to get C4GH keys secret".to_string()))
    }
  }

  async fn write_to_file(to: &Path, secret: impl Into<String>, client: &Client) -> Result<()> {
    let data = Self::get_secret(client, secret).await?;
    Ok(fs::write(to, data)?)
  }

  /// Get the client to use for fetching keys.
  pub async fn client(&self) -> Client {
    if let Some(client) = self.client.clone() {
      client
    } else {
      Client::new(&load_defaults(BehaviorVersion::latest()).await)
    }
  }

  /// Get the private key if this is a local private key.
  pub async fn into_private_key(self) -> Result<Vec<u8>> {
    let client = self.client().await;

    let tmp = NamedTempFile::new()?;
    Self::write_to_file(tmp.path(), self.key, &client).await?;

    Ok(get_private_key(
      tmp.path().to_path_buf(),
      Ok("".to_string()),
    )?)
  }

  /// Get the public key if this is a local public key.
  pub async fn into_public_key(self) -> Result<Vec<u8>> {
    let client = self.client().await;

    let tmp = NamedTempFile::new()?;
    Self::write_to_file(tmp.path(), self.key, &client).await?;

    Ok(get_public_key(tmp.path().to_path_buf())?)
  }
}

impl<T> From<SdkError<T>> for Error {
  fn from(err: SdkError<T>) -> Self {
    Error::IoError(err.to_string())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::storage::c4gh::{C4GHKeyLocation, C4GHKeySet, C4GHKeyType, C4GHKeys};
  use aws_sdk_secretsmanager::operation::get_secret_value::GetSecretValueOutput;
  use aws_sdk_secretsmanager::primitives::Blob;
  use aws_smithy_mocks::{Rule, RuleMode, mock, mock_client};
  use std::fs::read;
  use std::path::PathBuf;

  async fn test_get_keys(rules: &[&Rule]) {
    let client = mock_client!(aws_sdk_secretsmanager, RuleMode::Sequential, rules);

    let secrets_manager_private =
      C4GHSecretsManager::new("private_key".to_string()).with_client(client.clone());
    let secrets_manager_public =
      C4GHSecretsManager::new("server_public_key".to_string()).with_client(client.clone());
    let secrets_manager_client =
        C4GHSecretsManager::new("client_public_key".to_string()).with_client(client);
    let location = C4GHKeySet {
      server: C4GHKeyLocation {
        private: Some(C4GHKeyType::SecretsManager(secrets_manager_private)),
        public: C4GHKeyType::SecretsManager(secrets_manager_public),
      },
      client: C4GHKeyLocation {
        private: None,
        public: C4GHKeyType::SecretsManager(secrets_manager_client),
      }
    };
    let keys: C4GHKeys = location.try_into().unwrap();
    let (server_keys, client_keys, _) = keys.into_inner().await.unwrap();

    assert_eq!(server_keys.len(), 1);
    assert_eq!(client_keys.len(), 1);
  }

  #[tokio::test]
  async fn config_test_get_keys_string() {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .parent()
      .unwrap()
      .to_path_buf();

    let private_key = read(parent.join("data/c4gh/keys/bob.sec")).unwrap();
    let server_public_key = read(parent.join("data/c4gh/keys/bob.pub")).unwrap();
    let client_public_key = read(parent.join("data/c4gh/keys/alice.pub")).unwrap();

    let get_private_key = mock!(Client::get_secret_value)
      .match_requests(|req| req.secret_id() == Some("private_key"))
      .then_output(move || {
        GetSecretValueOutput::builder()
          .secret_string(String::from_utf8(private_key.clone()).unwrap())
          .build()
      });
    let get_recipient_public_key = mock!(Client::get_secret_value)
      .match_requests(|req| req.secret_id() == Some("server_public_key"))
      .then_output(move || {
        GetSecretValueOutput::builder()
          .secret_string(String::from_utf8(server_public_key.clone()).unwrap())
          .build()
      });
    let get_client_public_key = mock!(Client::get_secret_value)
        .match_requests(|req| req.secret_id() == Some("client_public_key"))
        .then_output(move || {
          GetSecretValueOutput::builder()
              .secret_string(String::from_utf8(client_public_key.clone()).unwrap())
              .build()
        });

    test_get_keys(&[&get_private_key, &get_recipient_public_key, &get_client_public_key]).await;
  }

  #[tokio::test]
  async fn config_test_get_keys_binary() {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .parent()
      .unwrap()
      .to_path_buf();

    let private_key = read(parent.join("data/c4gh/keys/bob.sec")).unwrap();
    let server_public_key = read(parent.join("data/c4gh/keys/bob.pub")).unwrap();
    let client_public_key = read(parent.join("data/c4gh/keys/alice.pub")).unwrap();

    let get_private_key = mock!(Client::get_secret_value)
      .match_requests(|req| req.secret_id() == Some("private_key"))
      .then_output(move || {
        GetSecretValueOutput::builder()
          .secret_binary(Blob::new(private_key.clone()))
          .build()
      });
    let get_recipient_public_key = mock!(Client::get_secret_value)
      .match_requests(|req| req.secret_id() == Some("server_public_key"))
      .then_output(move || {
        GetSecretValueOutput::builder()
          .secret_binary(Blob::new(server_public_key.clone()))
          .build()
      });
    let get_recipient_public_key = mock!(Client::get_secret_value)
        .match_requests(|req| req.secret_id() == Some("client_public_key"))
        .then_output(move || {
          GetSecretValueOutput::builder()
              .secret_binary(Blob::new(client_public_key.clone()))
              .build()
        });

    test_get_keys(&[&get_private_key, &get_recipient_public_key, &get_recipient_public_key]).await;
  }
}
