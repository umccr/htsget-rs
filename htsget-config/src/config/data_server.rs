//! Data server configuration.
//!

use crate::config::advanced::cors::CorsConfig;
use crate::storage::file::{default_localstorage_addr, default_path};
use crate::tls::TlsServerConfig;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// Tagged allow headers for cors config, either Mirror or Any.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum DataServerTagged {
  #[serde(alias = "none", alias = "NONE", alias = "null")]
  None,
}

/// Whether the data server is enabled or not.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged, deny_unknown_fields)]
#[allow(clippy::large_enum_variant)]
pub enum DataServerEnabled {
  None(DataServerTagged),
  Some(DataServerConfig),
}

impl DataServerEnabled {
  pub fn unwrap(self) -> DataServerConfig {
    if let Self::Some(config) = self {
      config
    } else {
      panic!("called `DataServerEnabled::unwrap()` on a `None` value")
    }
  }
}

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct DataServerConfig {
  addr: SocketAddr,
  local_path: PathBuf,
  #[serde(skip_serializing)]
  tls: Option<TlsServerConfig>,
  cors: CorsConfig,
}

impl DataServerConfig {
  /// Create the ticket server config.
  pub fn new(
    addr: SocketAddr,
    local_path: PathBuf,
    tls: Option<TlsServerConfig>,
    cors: CorsConfig,
  ) -> Self {
    Self {
      addr,
      local_path,
      tls,
      cors,
    }
  }

  /// Get the socket address.
  pub fn addr(&self) -> SocketAddr {
    self.addr
  }

  /// Get the local path.
  pub fn local_path(&self) -> &Path {
    self.local_path.as_path()
  }

  /// Get the TLS config.
  pub fn tls(&self) -> Option<&TlsServerConfig> {
    self.tls.as_ref()
  }

  /// Get the CORS config.
  pub fn cors(&self) -> &CorsConfig {
    &self.cors
  }

  pub fn into_tls(self) -> Option<TlsServerConfig> {
    self.tls
  }
}

impl Default for DataServerConfig {
  fn default() -> Self {
    Self {
      addr: default_localstorage_addr()
        .parse()
        .expect("expected valid address"),
      local_path: default_path().into(),
      tls: Default::default(),
      cors: Default::default(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::tests::test_serialize_and_deserialize;

  #[test]
  fn data_server() {
    test_serialize_and_deserialize(
      r#"
      addr = "127.0.0.1:8083"
      local_path = "path"
      cors.max_age = 1
      "#,
      ("127.0.0.1:8083".to_string(), "path".to_string(), 1),
      |result: DataServerConfig| {
        (
          result.addr().to_string(),
          result.local_path().to_string_lossy().to_string(),
          result.cors.max_age(),
        )
      },
    );
  }
}
