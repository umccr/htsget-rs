pub use htsget_config::config::{
  Config, DataServerConfig, ServiceInfo, StorageType, TicketServerConfig,
};
pub use htsget_config::regex_resolver::{HtsGetIdResolver, RegexResolver};

pub mod htsget;
pub mod storage;
