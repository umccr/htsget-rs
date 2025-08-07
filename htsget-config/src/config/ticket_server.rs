//! Ticket server configuration.
//!

use crate::config::advanced::auth::AuthConfig;
use crate::config::advanced::cors::CorsConfig;
use crate::tls::TlsServerConfig;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Configuration for the htsget ticket server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct TicketServerConfig {
  addr: SocketAddr,
  #[serde(skip_serializing)]
  tls: Option<TlsServerConfig>,
  cors: CorsConfig,
  #[serde(skip_serializing)]
  auth: Option<AuthConfig>,
}

impl TicketServerConfig {
  /// Create the ticket server config.
  pub fn new(
    addr: SocketAddr,
    tls: Option<TlsServerConfig>,
    cors: CorsConfig,
    auth: Option<AuthConfig>,
  ) -> Self {
    Self {
      addr,
      tls,
      cors,
      auth,
    }
  }

  /// Get the socket address.
  pub fn addr(&self) -> SocketAddr {
    self.addr
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

  /// Set the auth config.
  pub fn set_auth(&mut self, auth: Option<AuthConfig>) {
    self.auth = auth;
  }

  /// Get the owned TLS config.
  pub fn into_tls(self) -> Option<TlsServerConfig> {
    self.tls
  }
}

impl Default for TicketServerConfig {
  fn default() -> Self {
    Self {
      addr: default_addr().parse().expect("expected valid address"),
      tls: Default::default(),
      cors: Default::default(),
      auth: None,
    }
  }
}

fn default_addr() -> &'static str {
  "127.0.0.1:8080"
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
      cors.max_age = 1
      "#,
      ("127.0.0.1:8083".to_string(), 1),
      |result: TicketServerConfig| (result.addr().to_string(), result.cors.max_age()),
    );
  }
}
