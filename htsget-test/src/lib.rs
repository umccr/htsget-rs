#[cfg(any(feature = "server-tests", feature = "http"))]
pub use htsget_config::{
  config::{Config, DataServerConfig, ServiceInfo, TicketServerConfig},
  storage::Storage,
};

#[cfg(feature = "aws-mocks")]
pub mod aws_mocks;
#[cfg(feature = "http")]
pub mod http;
pub mod util;
