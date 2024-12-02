//! Data server configuration.
//!

use crate::config::{default_localstorage_addr, default_path};
use crate::tls::TlsServerConfig;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct DataServerConfig {
  addr: SocketAddr,
  local_path: PathBuf,
  #[serde(skip_serializing)]
  tls: Option<TlsServerConfig>,
}

impl DataServerConfig {
  /// Create the ticket server config.
  pub fn new(addr: SocketAddr, local_path: PathBuf, tls: Option<TlsServerConfig>) -> Self {
    Self {
      addr,
      local_path,
      tls,
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
}

impl Default for DataServerConfig {
  fn default() -> Self {
    Self {
      addr: default_localstorage_addr()
        .parse()
        .expect("expected valid address"),
      local_path: default_path().into(),
      tls: None,
    }
  }
}
