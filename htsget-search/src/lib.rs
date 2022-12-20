pub use htsget_config::config::{Config, DataServerConfig, ServiceInfo, TicketServerConfig};
#[cfg(feature = "s3-storage")]
pub use htsget_config::regex_resolver::aws::S3Resolver;
pub use htsget_config::regex_resolver::{
    QueryMatcher, RegexResolver, Resolver, StorageType, LocalResolver,
};

pub mod htsget;
pub mod storage;
