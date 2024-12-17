//! Local C4GH key storage.
//!

use crate::error::{Error, Result};
use crate::storage::c4gh::C4GHKeys;
use crypt4gh::keys::{get_private_key, get_public_key};
use serde::Deserialize;
use std::path::PathBuf;

/// Local C4GH key storage.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct C4GHLocal {
  private: PathBuf,
  public: PathBuf,
}

impl C4GHLocal {
  /// Create a new local C4GH key storage.
  pub fn new(private: PathBuf, public: PathBuf) -> Self {
    Self { private, public }
  }
}

impl TryFrom<C4GHLocal> for C4GHKeys {
  type Error = Error;

  fn try_from(local: C4GHLocal) -> Result<Self> {
    let private_key = get_private_key(local.private, Ok("".to_string()))?;
    let recipient_public_key = get_public_key(local.public)?;

    let handle =
      tokio::spawn(async move { Ok(C4GHKeys::from_key_pair(private_key, recipient_public_key)) });

    Ok(C4GHKeys::from_join_handle(handle))
  }
}

#[cfg(test)]
mod tests {
  use crate::config::tests::test_config_from_file;
  use crate::config::Config;
  use crate::storage::Backend;
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
        [[locations]]
        regex = "regex"

        [locations.backend]
        {}

        [locations.backend.keys]
        kind = "File"
        private = "{}"
        public = "{}"
        "#,
        storage_config,
        private_key.to_string_lossy(),
        recipient_public_key.to_string_lossy()
      ),
      |config| {
        test_fn(config);
      },
    );
  }
  #[tokio::test]
  async fn config_local_storage_c4gh() {
    test_c4gh_storage_config(r#"kind = "File""#, |config| {
      assert!(matches!(
            config.locations().first().unwrap().backend(),
            Backend::File(file) if file.keys().is_some()
      ));
    });
  }

  #[cfg(feature = "s3-storage")]
  #[tokio::test]
  async fn config_s3_storage_c4gh() {
    test_c4gh_storage_config(
      r#"
        kind = "S3"
        bucket = "bucket"
        "#,
      |config| {
        assert!(matches!(
              config.locations().first().unwrap().backend(),
              Backend::S3(s3) if s3.keys().is_some()
        ));
      },
    );
  }

  #[cfg(feature = "url-storage")]
  #[tokio::test]
  async fn config_url_storage_c4gh() {
    test_c4gh_storage_config(
      r#"
        kind = "Url"
        url = "https://example.com/"
        response_url = "https://example.com/"
        forward_headers = false
        "#,
      |config| {
        assert!(matches!(
              config.locations().first().unwrap().backend(),
              Backend::Url(url) if url.keys().is_some()
        ));
      },
    );
  }
}
