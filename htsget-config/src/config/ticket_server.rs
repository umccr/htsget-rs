//! Ticket server configuration.
//!

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
}

impl TicketServerConfig {
  /// Create the ticket server config.
  pub fn new(addr: SocketAddr, tls: Option<TlsServerConfig>) -> Self {
    Self { addr, tls }
  }

  /// Get the socket address.
  pub fn addr(&self) -> SocketAddr {
    self.addr
  }

  /// Get the TLS config.
  pub fn tls(&self) -> Option<&TlsServerConfig> {
    self.tls.as_ref()
  }
}

impl Default for TicketServerConfig {
  fn default() -> Self {
    Self {
      addr: default_addr().parse().expect("expected valid address"),
      tls: None,
    }
  }
}
