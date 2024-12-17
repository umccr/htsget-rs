#[cfg(any(feature = "url-storage", feature = "s3-storage"))]
use crate::error::Error;
use crate::error::Result;
use crate::storage::file::File;
#[cfg(feature = "s3-storage")]
use crate::storage::s3::S3;
#[cfg(feature = "url-storage")]
use crate::storage::url::Url;
use serde::{Deserialize, Serialize};

#[cfg(feature = "experimental")]
pub mod c4gh;
pub mod file;
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
#[serde(tag = "kind", deny_unknown_fields)]
#[non_exhaustive]
pub enum Backend {
  #[serde(alias = "file", alias = "FILE")]
  File(File),
  #[cfg(feature = "s3-storage")]
  #[serde(alias = "s3")]
  S3(S3),
  #[cfg(feature = "url-storage")]
  #[serde(alias = "url", alias = "URL")]
  Url(Url),
}

impl Backend {
  /// Get the file variant and error if it is not `File`.
  pub fn as_file(&self) -> Result<&File> {
    match self {
      Backend::File(file) => Ok(file),
      #[cfg(feature = "s3-storage")]
      Backend::S3(_) => Err(Error::ParseError("not a `File` variant".to_string())),
      #[cfg(feature = "url-storage")]
      Backend::Url(_) => Err(Error::ParseError("not a `File` variant".to_string())),
    }
  }

  /// Get the file variant and error if it is not `S3`.
  #[cfg(feature = "s3-storage")]
  pub fn as_s3(&self) -> Result<&S3> {
    if let Backend::S3(s3) = self {
      Ok(s3)
    } else {
      Err(Error::ParseError("not a `S3` variant".to_string()))
    }
  }

  /// Get the url variant and error if it is not `Url`.
  #[cfg(feature = "url-storage")]
  pub fn as_url(&self) -> Result<&Url> {
    if let Backend::Url(url) = self {
      Ok(url)
    } else {
      Err(Error::ParseError("not a `File` variant".to_string()))
    }
  }
}

impl Default for Backend {
  fn default() -> Self {
    Self::File(Default::default())
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use crate::config::tests::{test_config_from_env, test_config_from_file};
  use crate::storage::Backend;

  #[test]
  fn config_storage_tagged_local_file() {
    test_config_from_file(
      r#"
      [[locations]]
      regex = "regex"
      backend.kind = "File"
      "#,
      |config| {
        assert!(matches!(
          config.locations().first().unwrap().backend(),
          Backend::File { .. }
        ));
      },
    );
  }

  #[test]
  fn config_storage_tagged_local_env() {
    test_config_from_env(
      vec![("HTSGET_LOCATIONS", "[{backend={ kind=File }}]")],
      |config| {
        assert!(matches!(
          config.locations().first().unwrap().backend(),
          Backend::File { .. }
        ));
      },
    );
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn config_storage_tagged_s3_file() {
    test_config_from_file(
      r#"
      [[locations]]
      regex = "regex"
      backend.kind = "S3"
      "#,
      |config| {
        assert!(matches!(
          config.locations().first().unwrap().backend(),
          Backend::S3(..)
        ));
      },
    );
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn config_storage_tagged_s3_env() {
    test_config_from_env(
      vec![("HTSGET_LOCATIONS", "[{backend={ kind=S3 }}]")],
      |config| {
        assert!(matches!(
          config.locations().first().unwrap().backend(),
          Backend::S3(..)
        ));
      },
    );
  }
}
