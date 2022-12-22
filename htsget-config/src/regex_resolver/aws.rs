use serde;
use serde::{Deserialize, Serialize};

/// S3 configuration for the htsget server.
#[derive(Deserialize, Serialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(default)]
pub struct S3Resolver {
  bucket: String,
}

impl S3Resolver {
  /// Create a new S3 resolver.
  pub fn new(bucket: String) -> Self {
    Self { bucket }
  }

  /// Get the bucket.
  pub fn bucket(&self) -> &str {
    &self.bucket
  }
}
