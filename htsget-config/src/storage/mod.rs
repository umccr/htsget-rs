use crate::storage::local::LocalStorage;
#[cfg(feature = "s3-storage")]
use crate::storage::s3::S3Storage;
#[cfg(feature = "url-storage")]
use crate::storage::url::UrlStorageClient;
use serde::{Deserialize, Serialize};

pub mod local;
pub mod object;
#[cfg(feature = "s3-storage")]
pub mod s3;
#[cfg(feature = "url-storage")]
pub mod url;

/// A new type representing a resolved id.
#[derive(Debug)]
pub struct ResolvedId(String);

impl ResolvedId {
  /// Create a new resolved id.
  pub fn new(resolved_id: String) -> Self {
    Self(resolved_id)
  }

  /// Get the inner resolved id value.
  pub fn into_inner(self) -> String {
    self.0
  }
}

/// Specify the storage backend to use as config values.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "backend")]
#[non_exhaustive]
pub enum Storage {
  #[serde(alias = "local", alias = "LOCAL")]
  Local(LocalStorage),
  #[cfg(feature = "s3-storage")]
  #[serde(alias = "s3")]
  S3(S3Storage),
  #[cfg(feature = "url-storage")]
  #[serde(alias = "url", alias = "URL")]
  Url(#[serde(skip_serializing)] UrlStorageClient),
}

impl Default for Storage {
  fn default() -> Self {
    Self::Local(Default::default())
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use crate::config::tests::{test_config_from_env, test_config_from_file};

  use super::*;

  #[test]
  fn config_storage_tagged_local_file() {
    test_config_from_file(
      r#"
      [[resolvers]]
      [resolvers.storage]
      backend = "Local"
      regex = "regex"
      "#,
      |config| {
        println!("{:?}", config.resolvers().first().unwrap().storage());
        assert!(matches!(
          config.resolvers().first().unwrap().storage(),
          Storage::Local { .. }
        ));
      },
    );
  }

  #[test]
  fn config_storage_tagged_local_env() {
    test_config_from_env(
      vec![(
        "HTSGET_RESOLVERS",
        "[{storage={ backend=Local, use_data_server_config=true}}]",
      )],
      |config| {
        assert!(matches!(
          config.resolvers().first().unwrap().storage(),
          Storage::Local { .. }
        ));
      },
    );
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn config_storage_tagged_s3_file() {
    test_config_from_file(
      r#"
      [[resolvers]]
      [resolvers.storage]
      backend = "S3"
      regex = "regex"
      "#,
      |config| {
        println!("{:?}", config.resolvers().first().unwrap().storage());
        assert!(matches!(
          config.resolvers().first().unwrap().storage(),
          Storage::S3(..)
        ));
      },
    );
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn config_storage_tagged_s3_env() {
    test_config_from_env(
      vec![("HTSGET_RESOLVERS", "[{storage={ backend=S3 }}]")],
      |config| {
        assert!(matches!(
          config.resolvers().first().unwrap().storage(),
          Storage::S3(..)
        ));
      },
    );
  }
}
