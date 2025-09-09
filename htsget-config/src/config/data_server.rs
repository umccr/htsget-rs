//! Data server configuration.
//!

use crate::config::advanced::auth::AuthConfig;
use crate::config::advanced::cors::CorsConfig;
use crate::error::{Error::ParseError, Result};
use crate::http::TlsServerConfig;
use crate::storage::file::default_localstorage_addr;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// Tagged allow headers for cors config, either Mirror or Any.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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
  /// Get the data server config, or an error if `None`.
  pub fn as_data_server_config(&self) -> Result<&DataServerConfig> {
    if let Self::Some(config) = self {
      Ok(config)
    } else {
      Err(ParseError("expected `None` variant".to_string()))
    }
  }
}

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct DataServerConfig {
  addr: SocketAddr,
  local_path: Option<PathBuf>,
  #[serde(skip_serializing)]
  tls: Option<TlsServerConfig>,
  cors: CorsConfig,
  #[serde(skip_serializing)]
  auth: Option<AuthConfig>,
  ticket_origin: Option<String>,
}

impl DataServerConfig {
  /// Create the ticket server config.
  pub fn new(
    addr: SocketAddr,
    local_path: PathBuf,
    tls: Option<TlsServerConfig>,
    cors: CorsConfig,
    auth: Option<AuthConfig>,
    ticket_origin: Option<String>,
  ) -> Self {
    Self {
      addr,
      local_path: Some(local_path),
      tls,
      cors,
      auth,
      ticket_origin,
    }
  }

  /// Get the socket address.
  pub fn addr(&self) -> SocketAddr {
    self.addr
  }

  /// Get the local path.
  pub fn local_path(&self) -> Option<&Path> {
    self.local_path.as_deref()
  }

  /// Set the local path.
  pub fn set_local_path(&mut self, local_path: Option<PathBuf>) {
    self.local_path = local_path;
  }

  /// Get the TLS config.
  pub fn tls(&self) -> Option<&TlsServerConfig> {
    self.tls.as_ref()
  }

  /// Get the CORS config.
  pub fn cors(&self) -> &CorsConfig {
    &self.cors
  }

  /// Get the auth config.
  pub fn auth(&self) -> Option<&AuthConfig> {
    self.auth.as_ref()
  }

  /// Get the ticket origin.
  pub fn ticket_origin(&self) -> Option<String> {
    self.ticket_origin.clone()
  }

  /// Set the auth config.
  pub fn set_auth(&mut self, auth: Option<AuthConfig>) {
    self.auth = auth;
  }

  /// Get the owned TLS config.
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
      local_path: None,
      tls: Default::default(),
      cors: Default::default(),
      auth: None,
      ticket_origin: None,
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
      ticket_origin = "http://example.com"
      "#,
      (
        "127.0.0.1:8083".to_string(),
        "path".to_string(),
        1,
        "http://example.com".to_string(),
      ),
      |result: DataServerConfig| {
        (
          result.addr().to_string(),
          result.local_path().unwrap().to_string_lossy().to_string(),
          result.cors.max_age(),
          result.ticket_origin.unwrap(),
        )
      },
    );
  }
}
