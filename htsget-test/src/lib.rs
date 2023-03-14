#[cfg(any(feature = "cors-tests", feature = "server-tests"))]
pub use htsget_config::{
  config::{Config, DataServerConfig, ServiceInfo, TicketServerConfig},
  storage::Storage,
};

#[cfg(feature = "cors-tests")]
pub mod cors_tests;
#[cfg(feature = "http-tests")]
pub mod http_tests;
#[cfg(feature = "server-tests")]
pub mod server_tests;
pub mod util;
