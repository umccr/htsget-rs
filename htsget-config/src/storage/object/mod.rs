//! Defines the type of object used by storage.
//!

#[cfg(feature = "experimental")]
pub mod c4gh;

#[cfg(feature = "experimental")]
use crate::storage::object::c4gh::C4GHKeys;
use serde::{Deserialize, Serialize};

/// An object type, can be regular or Crypt4GH encrypted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectType {
  #[serde(skip_serializing, flatten)]
  #[cfg(feature = "experimental")]
  keys: Option<C4GHKeys>,
}

impl ObjectType {
  #[cfg(feature = "experimental")]
  /// Get the C4GH keys.
  pub fn keys(&self) -> Option<&C4GHKeys> {
    self.keys.as_ref()
  }
}
