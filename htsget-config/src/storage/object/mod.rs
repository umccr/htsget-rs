//! Defines the type of object used by storage.
//!

#[cfg(feature = "c4gh-experimental")]
pub mod c4gh;

#[cfg(feature = "c4gh-experimental")]
use crate::storage::object::c4gh::C4GHKeys;
use serde::{Deserialize, Serialize};

/// An object type, can be regular or Crypt4GH encrypted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
#[non_exhaustive]
pub enum ObjectType {
  #[default]
  Regular,
  #[cfg(feature = "c4gh-experimental")]
  C4GH {
    #[serde(flatten, skip_serializing)]
    keys: C4GHKeys,
  },
}
