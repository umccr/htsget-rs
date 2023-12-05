//! Config related to how htsget-rs treats files and objects. Used as part of a `Resolver`.
//!

use serde::{Deserialize, Serialize};

#[cfg(feature = "crypt4gh")]
use crate::tls::crypt4gh::Crypt4GH;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(untagged, deny_unknown_fields)]
#[non_exhaustive]
pub enum ObjectType {
  #[default]
  Regular,
  // Only valid for url storage.
  #[cfg(feature = "crypt4gh")]
  Crypt4GHGenerate,
  #[cfg(feature = "crypt4gh")]
  Crypt4GH {
    #[serde(flatten, skip_serializing)]
    crypt4gh: Crypt4GH,
  },
}

impl ObjectType {
  #[cfg(feature = "crypt4gh")]
  pub fn is_crypt4gh(&self) -> bool {
    matches!(self, ObjectType::Crypt4GH { .. })
  }
}
