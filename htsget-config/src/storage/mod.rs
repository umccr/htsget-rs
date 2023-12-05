use serde::{Deserialize, Serialize};

use crate::resolver::ResolveResponse;
use crate::storage::local::LocalStorage;
#[cfg(feature = "s3-storage")]
use crate::storage::s3::S3Storage;
#[cfg(feature = "url-storage")]
use crate::storage::url::UrlStorageClient;
use crate::types::{Query, Response, Result};

pub mod local;
#[cfg(feature = "s3-storage")]
pub mod s3;
#[cfg(feature = "url-storage")]
pub mod url;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TaggedStorageTypes {
  #[serde(alias = "local", alias = "LOCAL")]
  Local,
  #[cfg(feature = "s3-storage")]
  #[serde(alias = "s3")]
  S3,
}

/// If s3-storage is enabled, then the default is `S3`, otherwise it is `Local`.
impl Default for TaggedStorageTypes {
  #[cfg(not(feature = "s3-storage"))]
  fn default() -> Self {
    Self::Local
  }

  #[cfg(feature = "s3-storage")]
  fn default() -> Self {
    Self::S3
  }
}

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
#[serde(untagged, deny_unknown_fields)]
#[non_exhaustive]
pub enum Storage {
  Tagged(TaggedStorageTypes),
  Local {
    #[serde(flatten)]
    local_storage: LocalStorage,
  },
  #[cfg(feature = "s3-storage")]
  S3 {
    #[serde(flatten)]
    s3_storage: S3Storage,
  },
  #[cfg(feature = "url-storage")]
  Url {
    #[serde(flatten, skip_serializing)]
    url_storage: UrlStorageClient,
  },
}

impl Storage {
  /// Resolve the local component `Storage` into a type that implements `FromStorage`. Tagged
  /// `Local` storage is not resolved because it is resolved into untagged `Local` storage when
  /// `Config` is constructed.
  pub async fn resolve_local_storage<T: ResolveResponse>(
    &self,
    query: &Query,
  ) -> Option<Result<Response>> {
    match self {
      Storage::Local { local_storage } => Some(T::from_local(local_storage, query).await),
      _ => None,
    }
  }

  /// Resolve the s3 component of `Storage` into a type that implements `FromStorage`.
  #[cfg(feature = "s3-storage")]
  pub async fn resolve_s3_storage<T: ResolveResponse>(
    &self,
    first_match: Option<&str>,
    query: &Query,
  ) -> Option<Result<Response>> {
    match self {
      Storage::Tagged(TaggedStorageTypes::S3) => {
        let bucket = first_match?.to_string();

        let s3_storage = S3Storage::new(bucket, None, false);
        Some(T::from_s3(&s3_storage, query).await)
      }
      Storage::S3 { s3_storage } => Some(T::from_s3(s3_storage, query).await),
      _ => None,
    }
  }

  /// Resolve the url component of `Storage` into a type that implements `FromStorage`.
  #[cfg(feature = "url-storage")]
  pub async fn resolve_url_storage<T: ResolveResponse>(
    &self,
    query: &Query,
  ) -> Option<Result<Response>> {
    match self {
      Storage::Url { url_storage } => Some(T::from_url(url_storage, query).await),
      _ => None,
    }
  }
}

impl Default for Storage {
  fn default() -> Self {
    Self::Tagged(TaggedStorageTypes::default())
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
      regex = "regex"
      storage = "Local"
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
    test_config_from_env(vec![("HTSGET_RESOLVERS", "[{storage=Local}]")], |config| {
      assert!(matches!(
        config.resolvers().first().unwrap().storage(),
        Storage::Local { .. }
      ));
    });
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn default_tagged_storage_type_s3() {
    assert_eq!(TaggedStorageTypes::default(), TaggedStorageTypes::S3);
  }

  #[cfg(not(feature = "s3-storage"))]
  #[test]
  fn default_tagged_storage_type_local() {
    assert_eq!(TaggedStorageTypes::default(), TaggedStorageTypes::Local);
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn config_storage_tagged_s3_file() {
    test_config_from_file(
      r#"
      [[resolvers]]
      regex = "regex"
      storage = "S3"
      "#,
      |config| {
        println!("{:?}", config.resolvers().first().unwrap().storage());
        assert!(matches!(
          config.resolvers().first().unwrap().storage(),
          Storage::Tagged(TaggedStorageTypes::S3)
        ));
      },
    );
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn config_storage_tagged_s3_env() {
    test_config_from_env(vec![("HTSGET_RESOLVERS", "[{storage=S3}]")], |config| {
      assert!(matches!(
        config.resolvers().first().unwrap().storage(),
        Storage::Tagged(TaggedStorageTypes::S3)
      ));
    });
  }
}
