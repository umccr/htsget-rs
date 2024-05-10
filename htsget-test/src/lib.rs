#[cfg(feature = "http")]
pub use htsget_config::{
  config::{Config, DataServerConfig, ServiceInfo, TicketServerConfig},
  storage::Storage,
};

#[cfg(feature = "aws-mocks")]
pub mod aws_mocks;
pub mod error;
#[cfg(feature = "http")]
pub mod http;
pub mod util;
