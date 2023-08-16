//! Config related to how htsget-rs treats files and objects. Used as part of a `Resolver`.
//!

use serde::{Deserialize, Serialize};

/// Object type configuration.
#[derive(Serialize, Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ObjectType {
  #[cfg(feature = "crypt4gh")]
  is_crypt4gh: bool,
}

impl ObjectType {
  /// Get whether the object is Crypt4GH encrypted.
  #[cfg(feature = "crypt4gh")]
  pub fn is_crypt4gh(&self) -> bool {
    self.is_crypt4gh
  }
}
