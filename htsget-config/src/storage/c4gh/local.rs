//! Local C4GH key storage.
//!

use crate::error::{Error, Result};
use crate::storage::c4gh::C4GHKeys;
use crypt4gh::keys::{get_private_key, get_public_key};
use serde::Deserialize;
use std::path::PathBuf;

/// Local C4GH key storage.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct C4GHLocal {
  private_key: PathBuf,
  recipient_public_key: PathBuf,
}

impl C4GHLocal {
  /// Create a new local C4GH key storage.
  pub fn new(private_key: PathBuf, recipient_public_key: PathBuf) -> Self {
    Self {
      private_key,
      recipient_public_key,
    }
  }
}

impl TryFrom<C4GHLocal> for C4GHKeys {
  type Error = Error;

  fn try_from(local: C4GHLocal) -> Result<Self> {
    let private_key = get_private_key(local.private_key, Ok("".to_string()))?;
    let recipient_public_key = get_public_key(local.recipient_public_key)?;

    let handle =
      tokio::spawn(async move { Ok(C4GHKeys::from_key_pair(private_key, recipient_public_key)) });

    Ok(C4GHKeys::from_join_handle(handle))
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

        [resolvers.storage.keys]
        location = "Local"
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
  #[tokio::test]
  async fn config_local_storage_c4gh() {
    test_c4gh_storage_config(r#"backend = "Local""#, |config| {
      assert!(matches!(
            config.resolvers().first().unwrap().storage(),
            Storage::Local(local_storage) if local_storage.keys().is_some()
      ));
    });
  }

  #[cfg(feature = "s3-storage")]
  #[tokio::test]
  async fn config_s3_storage_c4gh() {
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
  #[tokio::test]
  async fn config_url_storage_c4gh() {
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
