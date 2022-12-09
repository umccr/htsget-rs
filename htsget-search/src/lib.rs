#[cfg(feature = "s3-storage")]
pub use htsget_config::config::aws::AwsS3DataServer;
pub use htsget_config::config::{
  Config, LocalDataServer, ServiceInfo, StorageType, TicketServerConfig,
};
pub use htsget_config::regex_resolver::{IdResolver, RegexResolver};

pub mod htsget;
pub mod storage;
