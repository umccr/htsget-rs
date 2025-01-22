//! Storage backends.
//!

#[cfg(any(feature = "url", feature = "aws"))]
use crate::error::Error;
use crate::error::Result;
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use crate::storage::file::File;
#[cfg(feature = "aws")]
use crate::storage::s3::S3;
#[cfg(feature = "url")]
use crate::storage::url::Url;
use serde::{Deserialize, Serialize};

#[cfg(feature = "experimental")]
pub mod c4gh;
pub mod file;
#[cfg(feature = "aws")]
pub mod s3;
#[cfg(feature = "url")]
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
  #[cfg(feature = "aws")]
  #[serde(alias = "s3")]
  S3(S3),
  #[cfg(feature = "url")]
  #[serde(alias = "url", alias = "URL")]
  Url(Url),
}

impl Backend {
  /// Get the file variant and error if it is not `File`.
  pub fn as_file(&self) -> Result<&File> {
    match self {
      Backend::File(file) => Ok(file),
      #[cfg(feature = "aws")]
      Backend::S3(_) => Err(Error::ParseError("not a `File` variant".to_string())),
      #[cfg(feature = "url")]
      Backend::Url(_) => Err(Error::ParseError("not a `File` variant".to_string())),
    }
  }

  /// Get the file variant and error if it is not `S3`.
  #[cfg(feature = "aws")]
  pub fn as_s3(&self) -> Result<&S3> {
    if let Backend::S3(s3) = self {
      Ok(s3)
    } else {
      Err(Error::ParseError("not a `S3` variant".to_string()))
    }
  }

  /// Get the url variant and error if it is not `Url`.
  #[cfg(feature = "url")]
  pub fn as_url(&self) -> Result<&Url> {
    if let Backend::Url(url) = self {
      Ok(url)
    } else {
      Err(Error::ParseError("not a `File` variant".to_string()))
    }
  }

  /// Set the C4GH keys.
  #[cfg(feature = "experimental")]
  pub fn set_keys(&mut self, keys: Option<C4GHKeys>) {
    match self {
      Backend::File(file) => file.set_keys(keys),
      #[cfg(feature = "aws")]
      Backend::S3(s3) => s3.set_keys(keys),
      #[cfg(feature = "url")]
      Backend::Url(url) => url.set_keys(keys),
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

  #[cfg(feature = "aws")]
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

  #[cfg(feature = "aws")]
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
