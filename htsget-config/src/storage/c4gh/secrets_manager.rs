//! Obtain C4GH keys from AWS secrets manager.
//!

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crate::storage::c4gh::C4GHKeys;
use aws_config::{load_defaults, BehaviorVersion};
use aws_sdk_secretsmanager::error::SdkError;
use aws_sdk_secretsmanager::Client;
use crypt4gh::keys::{get_private_key, get_public_key};
use crypt4gh::Keys;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// C4GH secrets manager key storage.
#[derive(Deserialize, Debug, Clone)]
pub struct C4GHSecretsManager {
  private_key: String,
  recipient_public_key: String,
  #[serde(skip)]
  client: Option<Client>,
}

impl C4GHSecretsManager {
  /// Create a new C4GH secrets manager key storage.
  pub fn new(private_key: String, recipient_public_key: String) -> Self {
    Self {
      private_key,
      recipient_public_key,
      client: None,
    }
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

  /// Retrieve the C4GH keys from secrets manager.
  pub async fn get_keys(self) -> Result<Vec<Keys>> {
    let client = if let Some(client) = self.client {
      client
    } else {
      Client::new(&load_defaults(BehaviorVersion::latest()).await)
    };

    // Should not have to do this, but the Crypt4GH library expects a path.
    let tmp = TempDir::new()?;
    let private_key = tmp.path().join("private_key");
    Self::write_to_file(&private_key, self.private_key, &client).await?;

    let recipient_public_key = tmp.path().join("public_key");
    Self::write_to_file(&recipient_public_key, self.recipient_public_key, &client).await?;

    let private_key = get_private_key(private_key, Ok("".to_string()))?;
    let recipient_public_key = get_public_key(recipient_public_key)?;

    Ok(C4GHKeys::from_key_pair(private_key, recipient_public_key))
  }
}

impl<T> From<SdkError<T>> for Error {
  fn from(err: SdkError<T>) -> Self {
    Error::IoError(err.to_string())
  }
}

impl TryFrom<C4GHSecretsManager> for C4GHKeys {
  type Error = Error;

  fn try_from(secrets_manager: C4GHSecretsManager) -> Result<Self> {
    Ok(C4GHKeys::from_join_handle(tokio::spawn(
      secrets_manager.get_keys(),
    )))
  }
}

#[cfg(test)]
mod tests {
  use aws_sdk_secretsmanager::operation::get_secret_value::GetSecretValueOutput;
  use aws_sdk_secretsmanager::primitives::Blob;
  use aws_smithy_mocks_experimental::{mock, mock_client, Rule, RuleMode};
  use std::fs::read;
  use std::path::PathBuf;

  use super::*;

  async fn test_get_keys(rules: &[&Rule]) {
    let client = mock_client!(aws_sdk_secretsmanager, RuleMode::Sequential, rules);

    let secrets_manager_config = C4GHSecretsManager::new(
      "private_key".to_string(),
      "recipient_public_key".to_string(),
    )
    .with_client(client);
    let keys: C4GHKeys = secrets_manager_config.try_into().unwrap();
    let keys = keys.keys().await.unwrap();

    assert_eq!(keys.len(), 1);
  }

  #[tokio::test]
  async fn config_test_get_keys_string() {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .parent()
      .unwrap()
      .to_path_buf();

    let private_key = read(parent.join("data/c4gh/keys/bob.sec")).unwrap();
    let recipient_public_key = read(parent.join("data/c4gh/keys/alice.pub")).unwrap();

    let get_private_key = mock!(Client::get_secret_value)
      .match_requests(|req| req.secret_id() == Some("private_key"))
      .then_output(move || {
        GetSecretValueOutput::builder()
          .secret_string(String::from_utf8(private_key.clone()).unwrap())
          .build()
      });
    let get_recipient_public_key = mock!(Client::get_secret_value)
      .match_requests(|req| req.secret_id() == Some("recipient_public_key"))
      .then_output(move || {
        GetSecretValueOutput::builder()
          .secret_string(String::from_utf8(recipient_public_key.clone()).unwrap())
          .build()
      });

    test_get_keys(&[&get_private_key, &get_recipient_public_key]).await;
  }

  #[tokio::test]
  async fn config_test_get_keys_binary() {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .parent()
      .unwrap()
      .to_path_buf();

    let private_key = read(parent.join("data/c4gh/keys/bob.sec")).unwrap();
    let recipient_public_key = read(parent.join("data/c4gh/keys/alice.pub")).unwrap();

    let get_private_key = mock!(Client::get_secret_value)
      .match_requests(|req| req.secret_id() == Some("private_key"))
      .then_output(move || {
        GetSecretValueOutput::builder()
          .secret_binary(Blob::new(private_key.clone()))
          .build()
      });
    let get_recipient_public_key = mock!(Client::get_secret_value)
      .match_requests(|req| req.secret_id() == Some("recipient_public_key"))
      .then_output(move || {
        GetSecretValueOutput::builder()
          .secret_binary(Blob::new(recipient_public_key.clone()))
          .build()
      });

    test_get_keys(&[&get_private_key, &get_recipient_public_key]).await;
  }
}
