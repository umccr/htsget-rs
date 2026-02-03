//! Local C4GH key storage.
//!

use crate::error::Result;
use crypt4gh::keys::{get_private_key, get_public_key};
use schemars::JsonSchema;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// Specify keys from a local file.
#[derive(JsonSchema, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct C4GHLocal {
  /// The path to the key.
  key: PathBuf,
}

impl C4GHLocal {
  /// Create a new local C4GH key storage.
  pub fn new(key: PathBuf) -> Self {
    Self { key }
  }

  /// Get the private key if this is a local private key.
  pub fn into_private_key(self) -> Result<Vec<u8>> {
    Ok(get_private_key(self.key, Ok("".to_string()))?)
  }

  /// Get the public key as an encoded string without decoding the inner base64 data.
  pub fn public_key_encoded(&self) -> Result<Vec<u8>> {
    Ok(fs::read(&self.key)?)
  }

  /// Get the public key if this is a local public key.
  pub fn into_public_key(self) -> Result<Vec<u8>> {
    Ok(get_public_key(self.key)?)
  }
}

#[cfg(test)]
mod tests {
  use crate::config::Config;
  use crate::config::tests::test_config_from_file;
  use crate::storage::Backend;
  use crate::storage::c4gh::tests::copy_c4gh_keys;
  use tempfile::TempDir;

  fn test_c4gh_storage_config<F>(storage_config: &str, test_fn: F)
  where
    F: Fn(Config),
  {
    let tmp = TempDir::new().unwrap();
    let (private_key, server_public_key, client_public_key) = copy_c4gh_keys(tmp.path());

    test_config_from_file(
      &format!(
        r#"
        [[locations]]
        regex = "regex"

        [locations.backend]
        {}

        [locations.backend.keys]
        server.private.kind = "File"
        server.private.key = "{}"
        server.public.kind = "File"
        server.public.key = "{}"
        client.public.kind = "File"
        client.public.key = "{}"
        "#,
        storage_config,
        private_key.to_string_lossy(),
        server_public_key.to_string_lossy(),
        client_public_key.to_string_lossy()
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

  #[cfg(feature = "aws")]
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

  #[cfg(feature = "url")]
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
