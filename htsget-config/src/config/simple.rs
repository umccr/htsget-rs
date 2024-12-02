//! Simplified configuration.
//!

use crate::config::data_server::DataServerConfig;
use crate::config::location::Location;
use crate::config::service_info::ServiceInfo;
use crate::config::ticket_server::TicketServerConfig;
use serde::{Deserialize, Serialize};

/// Simplified config.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct Config {
  ticket_server: TicketServerConfig,
  data_server: Option<DataServerConfig>,
  service_info: Option<ServiceInfo>,
  location: Option<Location>,
}

impl Config {
  /// Create a config.
  pub fn new(
    ticket_server: TicketServerConfig,
    data_server: Option<DataServerConfig>,
    service_info: Option<ServiceInfo>,
    location: Option<Location>,
  ) -> Self {
    Self {
      ticket_server,
      data_server,
      service_info,
      location,
    }
  }

  /// Get the ticket server config.
  pub fn ticket_server(&self) -> &TicketServerConfig {
    &self.ticket_server
  }

  /// Get the data server config.
  pub fn data_server(&self) -> Option<&DataServerConfig> {
    self.data_server.as_ref()
  }

  /// Get the service info config.
  pub fn service_info(&self) -> Option<&ServiceInfo> {
    self.service_info.as_ref()
  }

  /// Get the location.
  pub fn location(&self) -> Option<&Location> {
    self.location.as_ref()
  }
}
