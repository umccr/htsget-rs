//! Set the location using a regex and substitution values.
//!

use crate::config::advanced::file::File;
#[cfg(feature = "s3-storage")]
use crate::config::advanced::s3::S3;
#[cfg(feature = "url-storage")]
use crate::config::advanced::url::UrlStorage;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Specify the storage backend to use as config values.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "backend")]
#[non_exhaustive]
pub enum Backend {
  #[serde(alias = "local", alias = "LOCAL")]
  Local(File),
  #[cfg(feature = "s3-storage")]
  #[serde(alias = "s3")]
  S3(S3),
  #[cfg(feature = "url-storage")]
  #[serde(alias = "url", alias = "URL")]
  Url(UrlStorage),
}

impl Default for Backend {
  fn default() -> Self {
    Self::Local(Default::default())
  }
}

/// A regex storage is a storage that matches ids using Regex.
#[derive(Serialize, Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RegexLocation {
  #[serde(with = "serde_regex")]
  regex: Regex,
  substitution_string: String,
  storage: Backend,
}

impl RegexLocation {
  /// Create a new regex location.
  pub fn new(regex: Regex, substitution_string: String, storage: Backend) -> Self {
    Self {
      regex,
      substitution_string,
      storage,
    }
  }

  /// Get the regex.
  pub fn regex(&self) -> &Regex {
    &self.regex
  }

  /// Get the substitution string.
  pub fn substitution_string(&self) -> &str {
    &self.substitution_string
  }

  /// Get the storage backend.
  pub fn backend(&self) -> &Backend {
    &self.storage
  }
}

impl Default for RegexLocation {
  fn default() -> Self {
    Self::new(
      ".*".parse().expect("expected valid regex"),
      "$0".to_string(),
      Default::default(),
    )
  }
}
