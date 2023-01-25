use serde;
use serde::{Deserialize, Serialize};

/// GDS configuration for the htsget server.
#[derive(Deserialize, Serialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(default)]
pub struct GDSResolver {
  volume: String,
}

impl GDSResolver {
  /// Create a new GDS resolver.
  pub fn new(volume: String) -> Self {
    Self { volume }
  }

  /// Get the bucket.
  pub fn volume(&self) -> &str {
    &self.volume
  }
}
