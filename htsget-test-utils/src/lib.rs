#[cfg(feature = "s3-storage")]
pub use htsget_config::config::aws::AwsS3DataServer;
#[cfg(any(feature = "cors-tests", feature = "server-tests"))]
pub use htsget_config::config::{
  Config, LocalDataServer, ServiceInfo, StorageType, TicketServerConfig,
};

#[cfg(feature = "cors-tests")]
pub mod cors_tests;
#[cfg(feature = "http-tests")]
pub mod http_tests;
#[cfg(feature = "server-tests")]
pub mod server_tests;
pub mod util;
