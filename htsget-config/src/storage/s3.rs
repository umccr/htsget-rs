//! Configuration for storage on AWS S3.
//!

#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Configure the server to fetch data and return tickets from S3.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct S3 {
  /// The bucket to use.
  bucket: String,
  /// The S3 endpoint to use.
  endpoint: Option<String>,
  /// Whether path style or virtual host addressing should be used.
  path_style: bool,
  /// Optional Crypt4GH keys to use when decrypting data.
  #[cfg(feature = "experimental")]
  #[serde(skip_serializing)]
  keys: Option<C4GHKeys>,
  #[serde(skip)]
  pub(crate) is_defaulted: bool,
}

impl Eq for S3 {}

impl PartialEq for S3 {
  fn eq(&self, other: &Self) -> bool {
    self.bucket == other.bucket
      && self.endpoint == other.endpoint
      && self.path_style == other.path_style
  }
}

impl S3 {
  /// Create a new S3 storage.
  pub fn new(bucket: String, endpoint: Option<String>, path_style: bool) -> Self {
    Self {
      bucket,
      endpoint,
      path_style,
      #[cfg(feature = "experimental")]
      keys: None,
      is_defaulted: false,
    }
  }

  /// Get the bucket.
  pub fn bucket(&self) -> &str {
    &self.bucket
  }

  /// Set the bucket.
  pub fn with_bucket(mut self, bucket: String) -> Self {
    self.bucket = bucket;
    self
  }

  /// Get the endpoint
  pub fn endpoint(&self) -> Option<&str> {
    self.endpoint.as_deref()
  }

  /// Set the endpoint.
  pub fn with_endpoint(mut self, endpoint: String) -> Self {
    self.endpoint = Some(endpoint);
    self
  }

  /// Get the path style
  pub fn path_style(&self) -> bool {
    self.path_style
  }

  /// Set the path style.
  pub fn with_path_style(mut self, path_style: bool) -> Self {
    self.path_style = path_style;
    self
  }

  #[cfg(feature = "experimental")]
  /// Set the C4GH keys.
  pub fn set_keys(&mut self, keys: Option<C4GHKeys>) {
    self.keys = keys;
  }

  #[cfg(feature = "experimental")]
  /// Get the C4GH keys.
  pub fn keys(&self) -> Option<&C4GHKeys> {
    self.keys.as_ref()
  }
}

impl Default for S3 {
  fn default() -> Self {
    let mut s3 = Self::new(Default::default(), Default::default(), Default::default());
    s3.is_defaulted = true;
    s3
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::tests::test_serialize_and_deserialize;

  #[test]
  fn s3_backend() {
    test_serialize_and_deserialize(
      r#"
      bucket = "bucket"
      endpoint = "127.0.0.1:8083"
      path_style = true
      "#,
      ("127.0.0.1:8083".to_string(), "bucket".to_string(), true),
      |result: S3| {
        (
          result.endpoint.unwrap().to_string(),
          result.bucket.to_string(),
          result.path_style,
        )
      },
    );
  }
}
