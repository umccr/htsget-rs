//! Configuration of local file based storage.
//!

use crate::config::{default_localstorage_addr, default_path};
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use crate::types::Scheme;
use http::uri::Authority;
use serde::{Deserialize, Serialize};

/// Local file based storage.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct File {
  scheme: Scheme,
  #[serde(with = "http_serde::authority")]
  authority: Authority,
  local_path: String,
  #[serde(skip_serializing)]
  #[cfg(feature = "experimental")]
  keys: Option<C4GHKeys>,
}

impl File {
  /// Create a new local storage.
  pub fn new(scheme: Scheme, authority: Authority, local_path: String) -> Self {
    Self {
      scheme,
      authority,
      local_path,
      #[cfg(feature = "experimental")]
      keys: None,
    }
  }

  /// Get the scheme.
  pub fn scheme(&self) -> Scheme {
    self.scheme
  }

  /// Get the authority.
  pub fn authority(&self) -> &Authority {
    &self.authority
  }

  /// Get the local path.
  pub fn local_path(&self) -> &str {
    &self.local_path
  }

  #[cfg(feature = "experimental")]
  /// Set the C4GH keys.
  pub fn set_keys(mut self, keys: Option<C4GHKeys>) -> Self {
    self.keys = keys;
    self
  }

  #[cfg(feature = "experimental")]
  /// Get the C4GH keys.
  pub fn keys(&self) -> Option<&C4GHKeys> {
    self.keys.as_ref()
  }
}

impl Default for File {
  fn default() -> Self {
    Self::new(Scheme::Http, default_authority(), default_local_path())
  }
}

fn default_authority() -> Authority {
  Authority::from_static(default_localstorage_addr())
}

fn default_local_path() -> String {
  default_path().into()
}
