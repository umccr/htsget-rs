//! Configuration of local file based storage.
//!

use crate::config::data_server::DataServerConfig;
use crate::error::Error;
use crate::error::Error::ParseError;
use crate::error::Result;
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use crate::tls::KeyPairScheme;
use crate::types::Scheme;
use http::uri::Authority;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Local file based storage.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct File {
  scheme: Scheme,
  #[serde(with = "http_serde::authority")]
  authority: Authority,
  local_path: String,
  #[cfg(feature = "experimental")]
  #[serde(skip_serializing)]
  keys: Option<C4GHKeys>,
}

impl File {
  /// Create a new local storage.
  pub fn new(scheme: Scheme, authority: Authority, local_path: String) -> Self {
    Self {
      scheme,
      authority,
      local_path,
      ..Default::default()
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
  pub fn set_keys(&mut self, keys: Option<C4GHKeys>) {
    self.keys = keys;
  }

  #[cfg(feature = "experimental")]
  /// Get the C4GH keys.
  pub fn keys(&self) -> Option<&C4GHKeys> {
    self.keys.as_ref()
  }

  /// Set the local path.
  pub fn set_local_path(mut self, local_path: String) -> Self {
    self.local_path = local_path;
    self
  }
}

impl Default for File {
  fn default() -> Self {
    Self::new(Scheme::Http, default_authority(), default_path().into())
  }
}

impl TryFrom<&DataServerConfig> for File {
  type Error = Error;

  fn try_from(config: &DataServerConfig) -> Result<Self> {
    Ok(Self::new(
      config.tls().get_scheme(),
      Authority::from_str(&config.addr().to_string()).map_err(|err| ParseError(err.to_string()))?,
      config.local_path().to_string_lossy().to_string(),
    ))
  }
}

pub(crate) fn default_authority() -> Authority {
  Authority::from_static(default_localstorage_addr())
}

pub(crate) fn default_localstorage_addr() -> &'static str {
  "127.0.0.1:8081"
}

pub(crate) fn default_path() -> &'static str {
  "./"
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::tests::test_serialize_and_deserialize;

  #[test]
  fn file_backend() {
    test_serialize_and_deserialize(
      r#"
      scheme = "Https"
      authority = "127.0.0.1:8083"
      local_path = "path"
      "#,
      (
        "127.0.0.1:8083".to_string(),
        Scheme::Https,
        "path".to_string(),
      ),
      |result: File| {
        (
          result.authority.to_string(),
          result.scheme,
          result.local_path,
        )
      },
    );
  }
}
