use serde;
use serde::{Deserialize, Serialize};

/// Configuration for the htsget server.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(default)]
pub struct S3Resolver {
  bucket: String,
}

impl S3Resolver {
  pub fn bucket(&self) -> &str {
    &self.bucket
  }
}
