use std::io;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::PathBuf;

use serde::Deserialize;
use tracing::info;
use tracing::instrument;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{fmt, EnvFilter, Registry};

use crate::config::StorageType::LocalStorage;
use crate::regex_resolver::RegexResolver;

/// Represents a usage string for htsget-rs.
pub const USAGE: &str = r#"
The HtsGet server executables don't use command line arguments, but there are some environment variables that can be set to configure them:
* HTSGET_ADDR: The socket address for the server which creates response tickets. Default: "127.0.0.1:8080".
* HTSGET_PATH: The path to the directory where the server should be started. Default: "data". Unused if HTSGET_STORAGE_TYPE is "AwsS3Storage".
* HTSGET_REGEX: The regular expression that should match an ID. Default: ".*".
For more information about the regex options look in the documentation of the regex crate(https://docs.rs/regex/).
* HTSGET_SUBSTITUTION_STRING: The replacement expression. Default: "$0".
* HTSGET_STORAGE_TYPE: Either "LocalStorage" or "AwsS3Storage", representing which storage type to use. Default: "LocalStorage".

The following options are used for the ticket server.
* HTSGET_TICKET_SERVER_ADDR: The socket address to use for the server which responds to tickets. Default: "127.0.0.1:8081". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_TICKET_SERVER_KEY: The path to the PEM formatted X.509 private key used by the ticket response server. Default: "None". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_TICKET_SERVER_CERT: The path to the PEM formatted X.509 certificate used by the ticket response server. Default: "None". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".

The following options are used to configure AWS S3 storage.
* HTSGET_S3_BUCKET: The name of the AWS S3 bucket. Default: "". Unused if HTSGET_STORAGE_TYPE is not "AwsS3Storage".

The next variables are used to configure the info for the service-info endpoints.
* HTSGET_ID: The id of the service. Default: "None".
* HTSGET_NAME: The name of the service. Default: "None".
* HTSGET_VERSION: The version of the service. Default: "None".
* HTSGET_ORGANIZATION_NAME: The name of the organization. Default: "None".
* HTSGET_ORGANIZATION_URL: The url of the organization. Default: "None".
* HTSGET_CONTACT_URL: A url to provide contact to the users. Default: "None".
* HTSGET_DOCUMENTATION_URL: A link to the documentation. Default: "None".
* HTSGET_CREATED_AT: Date of the creation of the service. Default: "None".
* HTSGET_UPDATED_AT: Date of the last update of the service. Default: "None".
* HTSGET_ENVIRONMENT: The environment in which the service is running. Default: "None".
"#;

const ENVIRONMENT_VARIABLE_PREFIX: &str = "HTSGET_";

fn default_localstorage_addr() -> SocketAddr {
  "127.0.0.1:8081".parse().expect("expected valid address")
}

fn default_addr() -> SocketAddr {
  "127.0.0.1:8080".parse().expect("expected valid address")
}

fn default_path() -> PathBuf {
  PathBuf::from("data")
}

/// Specify the storage type to use.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StorageType {
  LocalStorage,
  #[cfg(feature = "s3-storage")]
  AwsS3Storage,
}

/// Configuration for the server. Each field will be read from environment variables
#[derive(Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Config {
  pub addr: SocketAddr,
  #[serde(flatten)]
  pub resolver: RegexResolver,
  pub path: PathBuf,
  #[serde(flatten)]
  pub service_info: ServiceInfo,
  pub storage_type: StorageType,
  pub ticket_server_addr: SocketAddr,
  pub ticket_server_key: Option<PathBuf>,
  pub ticket_server_cert: Option<PathBuf>,
  #[cfg(feature = "s3-storage")]
  pub s3_bucket: String,
}

/// Configuration of the service info.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct ServiceInfo {
  pub id: Option<String>,
  pub name: Option<String>,
  pub version: Option<String>,
  pub organization_name: Option<String>,
  pub organization_url: Option<String>,
  pub contact_url: Option<String>,
  pub documentation_url: Option<String>,
  pub created_at: Option<String>,
  pub updated_at: Option<String>,
  pub environment: Option<String>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      addr: default_addr(),
      resolver: RegexResolver::default(),
      path: default_path(),
      service_info: ServiceInfo::default(),
      storage_type: LocalStorage,
      ticket_server_addr: default_localstorage_addr(),
      ticket_server_key: None,
      ticket_server_cert: None,
      #[cfg(feature = "s3-storage")]
      s3_bucket: "".to_string(),
    }
  }
}

impl Config {
  /// Read the environment variables into a Config struct.
  #[instrument]
  pub fn from_env() -> io::Result<Self> {
    let config = envy::prefixed(ENVIRONMENT_VARIABLE_PREFIX)
      .from_env()
      .map_err(|err| {
        std::io::Error::new(
          ErrorKind::Other,
          format!("config not properly set: {}", err),
        )
      });
    info!(config = ?config, "config created from environment variables");
    config
  }

  /// Setup tracing, using a global subscriber.
  pub fn setup_tracing() -> io::Result<()> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = fmt::Layer::default();

    let subscriber = Registry::default().with(env_filter).with(fmt_layer);

    tracing::subscriber::set_global_default(subscriber).map_err(|err| {
      io::Error::new(
        ErrorKind::Other,
        format!("failed to install `tracing` subscriber: {}", err),
      )
    })?;

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn config_addr() {
    std::env::set_var("HTSGET_ADDR", "127.0.0.1:8081");
    let config = Config::from_env().unwrap();
    assert_eq!(config.addr, "127.0.0.1:8081".parse().unwrap());
  }

  #[test]
  fn config_ticket_server_addr() {
    std::env::set_var("HTSGET_TICKET_SERVER_ADDR", "127.0.0.1:8082");
    let config = Config::from_env().unwrap();
    assert_eq!(config.ticket_server_addr, "127.0.0.1:8082".parse().unwrap());
  }

  #[test]
  fn config_regex() {
    std::env::set_var("HTSGET_REGEX", ".+");
    let config = Config::from_env().unwrap();
    assert_eq!(config.resolver.regex.to_string(), ".+");
  }

  #[test]
  fn config_substitution_string() {
    std::env::set_var("HTSGET_SUBSTITUTION_STRING", "$0-test");
    let config = Config::from_env().unwrap();
    assert_eq!(config.resolver.substitution_string, "$0-test");
  }

  #[test]
  fn config_service_info_id() {
    std::env::set_var("HTSGET_ID", "id");
    let config = Config::from_env().unwrap();
    assert_eq!(config.service_info.id.unwrap(), "id");
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn config_storage_type() {
    std::env::set_var("HTSGET_STORAGE_TYPE", "AwsS3Storage");
    let config = Config::from_env().unwrap();
    assert_eq!(config.storage_type, StorageType::AwsS3Storage);
  }
}
