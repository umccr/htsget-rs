//! Location configuration.
//!

use crate::config::advanced::regex_location::RegexLocation;
use crate::config::location::Backend::{File, Url, S3};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// Either simple or regex based location
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum LocationEither {
  Simple(Location),
  Regex(RegexLocation),
}

impl Default for LocationEither {
  fn default() -> Self {
    Self::Simple(Default::default())
  }
}

/// Location config.
#[derive(Serialize, Debug, Clone, Default)]
#[serde(default)]
pub struct Location {
  backend: Backend,
  prefix: String,
}

impl Location {
  /// Create a new location.
  pub fn new(backend: Backend, prefix: String) -> Self {
    Self { backend, prefix }
  }

  /// Get the storage backend.
  pub fn backend(&self) -> Backend {
    self.backend
  }

  /// Get the prefix.
  pub fn prefix(&self) -> &str {
    &self.prefix
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, Copy)]
pub enum Backend {
  #[default]
  File,
  S3,
  Url,
}

impl<'de> Deserialize<'de> for Location {
  fn deserialize<D>(deserializer: D) -> Result<Location, D::Error>
  where
    D: Deserializer<'de>,
  {
    let s = String::deserialize(deserializer)?.to_lowercase();

    let endpoint = if s.strip_prefix("file://").is_some() {
      File
    } else if s.strip_prefix("s3://").is_some() {
      S3
    } else if s
      .strip_prefix("http://")
      .or_else(|| s.strip_prefix("https://"))
      .is_some()
    {
      Url
    } else {
      return Err(Error::custom(
        "expected file://, s3://, http:// or https:// scheme",
      ));
    };

    Ok(Location::new(endpoint, s))
  }
}
