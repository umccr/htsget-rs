//! Location configuration.
//!

use crate::config::location::Endpoint::{File, Url, S3};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// Location config.
#[derive(Serialize, Debug, Clone, Default)]
#[serde(default)]
pub struct Location {
  endpoint: Endpoint,
  prefix: String,
}

impl Location {
  /// Create a new location.
  pub fn new(endpoint: Endpoint, prefix: String) -> Self {
    Self { endpoint, prefix }
  }

  /// Get the endpoint.
  pub fn endpoint(&self) -> Endpoint {
    self.endpoint
  }

  /// Get the prefix.
  pub fn prefix(&self) -> &str {
    &self.prefix
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, Copy)]
pub enum Endpoint {
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
