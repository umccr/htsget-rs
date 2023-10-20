//! Config related to how htsget-rs treats files and objects. Used as part of a `Resolver`.
//!

#[cfg(feature = "crypt4gh")]
pub mod crypt4gh;

use crate::resolver::object::crypt4gh::Crypt4GHObject;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(untagged, deny_unknown_fields)]
#[non_exhaustive]
pub enum ObjectType {
  #[default]
  Regular,
  #[cfg(feature = "crypt4gh")]
  Crypt4GH {
    #[serde(flatten)]
    crypt4gh: Crypt4GHObject,
  },
}

impl ObjectType {
  #[cfg(feature = "crypt4gh")]
  pub fn is_crypt4gh(&self) -> bool {
    matches!(self, ObjectType::Crypt4GH { .. })
  }
}
