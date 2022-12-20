use serde;
use serde::{Deserialize, Serialize};

/// S3 configuration for the htsget server.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(default)]
pub struct S3Resolver {
  bucket: String,
}

impl S3Resolver {
  /// Get the bucket.
  pub fn bucket(&self) -> &str {
    &self.bucket
  }
}
