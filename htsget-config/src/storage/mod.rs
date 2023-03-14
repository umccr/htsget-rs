pub mod local;
#[cfg(feature = "s3-storage")]
pub mod s3;

use serde::{Deserialize, Serialize};

use crate::config::DataServerConfig;
use crate::storage::local::LocalStorage;
#[cfg(feature = "s3-storage")]
use crate::storage::s3::S3Storage;

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

/// A trait for converting a type from `Storage`.
pub trait FromStorage<T> {
  /// Convert from `LocalStorage`.
  fn from_local(local_storage: &LocalStorage) -> T;

  /// Convert from `S3Storage`.
  #[cfg(feature = "s3-storage")]
  fn from_s3_storage(s3_storage: &S3Storage) -> T;
}

/// Specify the storage backend to use as config values.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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
}

impl Storage {
  /// Resolve the local component `Storage` into a type that implements `FromStorage`.
  pub fn resolve_local_storage<T: FromStorage<T>>(&self, config: &DataServerConfig) -> Option<T> {
    match self {
      Storage::Tagged(TaggedStorageTypes::Local) => {
        let local_storage: Option<LocalStorage> = config.into();
        Some(T::from_local(&local_storage?))
      }
      Storage::Local { local_storage } => Some(T::from_local(local_storage)),
      #[cfg(feature = "s3-storage")]
      _ => None,
    }
  }

  /// Resolve the s3 component of `Storage` into a type that implements `FromStorage`.
  #[cfg(feature = "s3-storage")]
  pub fn resolve_s3_storage<T: FromStorage<T>>(&self, resolved_id: String) -> Option<T> {
    match self {
      Storage::Tagged(TaggedStorageTypes::S3) => {
        let s3_storage: Option<S3Storage> = ResolvedId(resolved_id).into();
        Some(T::from_s3_storage(&s3_storage?))
      }
      Storage::S3 { s3_storage } => Some(T::from_s3_storage(s3_storage)),
      _ => None,
    }
  }
}

impl Default for Storage {
  fn default() -> Self {
    Self::Tagged(TaggedStorageTypes::Local)
  }
}

#[cfg(test)]
pub mod tests {
  use super::*;
  use crate::config::tests::{test_config_from_env, test_config_from_file};

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
          Storage::Tagged(TaggedStorageTypes::Local)
        ));
      },
    );
  }

  #[test]
  fn config_storage_tagged_local_env() {
    test_config_from_env(vec![("HTSGET_RESOLVERS", "[{storage=Local}]")], |config| {
      assert!(matches!(
        config.resolvers().first().unwrap().storage(),
        Storage::Tagged(TaggedStorageTypes::Local)
      ));
    });
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
