//! Config related to how htsget-rs treats files and objects. Used as part of a `Resolver`.
//!

use serde::{Deserialize, Serialize};

#[cfg(feature = "crypt4gh")]
use crate::tls::crypt4gh::Crypt4GHKeyPair;

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
#[non_exhaustive]
pub enum ObjectType {
  #[default]
  Regular,
  #[cfg(feature = "crypt4gh")]
  // Only valid for url storage.
  GenerateKeys { send_encrypted_to_client: bool },
  #[cfg(feature = "crypt4gh")]
  Crypt4GH {
    send_encrypted_to_client: bool,
    #[serde(flatten, skip_serializing)]
    crypt4gh: Crypt4GHKeyPair,
  },
}

impl ObjectType {
  #[cfg(feature = "crypt4gh")]
  pub fn is_crypt4gh(&self) -> bool {
    match self {
      #[cfg(feature = "url-storage")]
      ObjectType::GenerateKeys { .. } => true,
      ObjectType::Crypt4GH { .. } => true,
      _ => false,
    }
  }

  /// Should returned data be unencrypted for the client.
  #[cfg(feature = "crypt4gh")]
  pub fn send_encrypted_to_client(&self) -> Option<bool> {
    match self {
      #[cfg(feature = "url-storage")]
      ObjectType::GenerateKeys {
        send_encrypted_to_client,
      } => Some(*send_encrypted_to_client),
      ObjectType::Crypt4GH {
        send_encrypted_to_client,
        ..
      } => Some(*send_encrypted_to_client),
      _ => None,
    }
  }

  #[cfg(feature = "crypt4gh")]
  pub fn crypt4gh_key_pair(&self) -> Option<&Crypt4GHKeyPair> {
    match self {
      ObjectType::Crypt4GH { crypt4gh, .. } => Some(crypt4gh),
      _ => None,
    }
  }
}
