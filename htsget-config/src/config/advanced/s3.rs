#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct S3 {
  bucket: String,
  endpoint: Option<String>,
  path_style: bool,
  #[serde(skip_serializing)]
  #[cfg(feature = "experimental")]
  keys: Option<C4GHKeys>,
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
    }
  }

  /// Get the bucket.
  pub fn bucket(&self) -> &str {
    &self.bucket
  }

  /// Get the endpoint
  pub fn endpoint(&self) -> Option<&str> {
    self.endpoint.as_deref()
  }

  /// Get the path style
  pub fn path_style(&self) -> bool {
    self.path_style
  }

  #[cfg(feature = "experimental")]
  /// Set the C4GH keys.
  pub fn set_keys(mut self, keys: Option<C4GHKeys>) -> Self {
    self.keys = keys;
    self
  }

  #[cfg(feature = "experimental")]
  /// Get the C4GH keys.
  pub fn keys(&self) -> Option<&C4GHKeys> {
    self.keys.as_ref()
  }
}
