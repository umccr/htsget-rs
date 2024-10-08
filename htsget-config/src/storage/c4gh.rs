//! Crypt4GH key parsing.
//!

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crypt4gh::error::Crypt4GHError;
use crypt4gh::keys::{get_private_key, get_public_key};
use crypt4gh::Keys;
use serde::Deserialize;
use std::path::PathBuf;

/// Config for Crypt4GH keys.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "C4GHPath")]
pub struct C4GHKeys {
  keys: Vec<Keys>,
}

impl C4GHKeys {
  /// Get the inner value.
  pub fn into_inner(self) -> Vec<Keys> {
    self.keys
  }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct C4GHPath {
  private_key: PathBuf,
  recipient_public_key: PathBuf,
}

impl C4GHPath {
  pub fn new(private_key: PathBuf, recipient_public_key: PathBuf) -> Self {
    Self {
      private_key,
      recipient_public_key,
    }
  }
}

impl TryFrom<C4GHPath> for C4GHKeys {
  type Error = Error;

  fn try_from(path: C4GHPath) -> Result<Self> {
    let private_key = get_private_key(path.private_key, Ok("".to_string()))?;
    let recipient_public_key = get_public_key(path.recipient_public_key)?;

    Ok(C4GHKeys {
      keys: vec![Keys {
        method: 0,
        privkey: private_key,
        recipient_pubkey: recipient_public_key,
      }],
    })
  }
}

impl From<Crypt4GHError> for Error {
  fn from(err: Crypt4GHError) -> Self {
    ParseError(err.to_string())
  }
}

#[cfg(test)]
mod tests {
  use crate::config::tests::test_config_from_file;
  use crate::config::Config;
  use crate::storage::Storage;
  use std::fs::copy;
  use std::path::PathBuf;
  use tempfile::TempDir;

  fn test_c4gh_storage_config<F>(storage_config: &str, test_fn: F)
  where
    F: Fn(Config),
  {
    let tmp = TempDir::new().unwrap();
    let private_key = tmp.path().join("bob.sec");
    let recipient_public_key = tmp.path().join("alice.pub");

    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .parent()
      .unwrap()
      .to_path_buf();

    copy(parent.join("data/c4gh/keys/bob.sec"), &private_key).unwrap();
    copy(
      parent.join("data/c4gh/keys/alice.pub"),
      &recipient_public_key,
    )
    .unwrap();

    test_config_from_file(
      &format!(
        r#"
        [[resolvers]]
        regex = "regex"

        [resolvers.storage]
        {}
        private_key = "{}"
        recipient_public_key = "{}"
        "#,
        storage_config,
        private_key.to_string_lossy(),
        recipient_public_key.to_string_lossy()
      ),
      |config| {
        println!("{:?}", config.resolvers().first().unwrap().storage());
        test_fn(config);
      },
    );
  }

  #[test]
  fn config_local_storage_c4gh() {
    test_c4gh_storage_config(r#"backend = "Local""#, |config| {
      assert!(matches!(
            config.resolvers().first().unwrap().storage(),
            Storage::Local(local_storage) if local_storage.keys().is_some()
      ));
    });
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn config_s3_storage_c4gh() {
    test_c4gh_storage_config(
      r#"
        backend = "S3"
        bucket = "bucket"
        "#,
      |config| {
        assert!(matches!(
              config.resolvers().first().unwrap().storage(),
              Storage::S3(s3_storage) if s3_storage.keys().is_some()
        ));
      },
    );
  }

  #[cfg(feature = "url-storage")]
  #[test]
  fn config_url_storage_c4gh() {
    test_c4gh_storage_config(
      r#"
        backend = "Url"
        url = "https://example.com/"
        response_url = "https://example.com/"
        forward_headers = false
        "#,
      |config| {
        assert!(matches!(
              config.resolvers().first().unwrap().storage(),
              Storage::Url(url_storage) if url_storage.keys().is_some()
        ));
      },
    );
  }
}
