//! Config related to how htsget-rs treats files and objects. Used as part of a `Resolver`.
//!

use serde::{Deserialize, Serialize};

#[cfg(feature = "crypt4gh")]
use crate::tls::crypt4gh::Crypt4GHKeyPair;

/// Tagged types. For now this is only for generating Crypt4GH keys.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TaggedObjectTypes {
  #[cfg(all(feature = "crypt4gh", feature = "url-storage"))]
  #[serde(alias = "generatekeys", alias = "GENERATEKEYS")]
  GenerateKeys,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
#[non_exhaustive]
pub enum ObjectType {
  #[default]
  Regular,
  #[cfg(feature = "crypt4gh")]
  // Only valid for url storage.
  Tagged(TaggedObjectTypes),
  #[cfg(feature = "crypt4gh")]
  Crypt4GH {
    #[serde(flatten, skip_serializing)]
    crypt4gh: Crypt4GHKeyPair,
  },
}

impl ObjectType {
  #[cfg(feature = "crypt4gh")]
  pub fn is_crypt4gh(&self) -> bool {
    match self {
      #[cfg(feature = "url-storage")]
      ObjectType::Tagged(TaggedObjectTypes::GenerateKeys) => true,
      ObjectType::Crypt4GH { .. } => true,
      _ => false,
    }
  }

  #[cfg(feature = "crypt4gh")]
  pub fn crypt4gh_key_pair(&self) -> Option<&Crypt4GHKeyPair> {
    match self {
      ObjectType::Crypt4GH { crypt4gh } => Some(crypt4gh),
      _ => None,
    }
  }
}
