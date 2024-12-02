//! Ticket server configuration.
//!

use crate::config::advanced::cors::CorsConfig;
use crate::config::default_addr;
use crate::tls::TlsServerConfig;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Configuration for the htsget ticket server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct TicketServerConfig {
  addr: SocketAddr,
  #[serde(skip_serializing)]
  tls: Option<TlsServerConfig>,
  cors: CorsConfig,
}

impl TicketServerConfig {
  /// Create the ticket server config.
  pub fn new(addr: SocketAddr, tls: Option<TlsServerConfig>, cors: CorsConfig) -> Self {
    Self { addr, tls, cors }
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
}

impl Default for TicketServerConfig {
  fn default() -> Self {
    Self {
      addr: default_addr().parse().expect("expected valid address"),
      tls: Default::default(),
      cors: Default::default(),
    }
  }
}
