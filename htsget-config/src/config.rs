use std::io;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use config::File;
use serde::Deserialize;
use tracing::info;
use tracing::instrument;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{fmt, EnvFilter, Registry};

use crate::config::StorageType::LocalStorage;
use crate::regex_resolver::RegexResolver;

/// Represents a usage string for htsget-rs.
pub const USAGE: &str = r#"
Available environment variables:
* HTSGET_PATH: The path to the directory where the server should be started. Default: "data". Unused if HTSGET_STORAGE_TYPE is "AwsS3Storage".
* HTSGET_REGEX: The regular expression that should match an ID. Default: ".*".
For more information about the regex options look in the documentation of the regex crate(https://docs.rs/regex/).
* HTSGET_SUBSTITUTION_STRING: The replacement expression. Default: "$0".
* HTSGET_STORAGE_TYPE: Either "LocalStorage" or "AwsS3Storage", representing which storage type to use. Default: "LocalStorage".

The following options are used for the ticket server.
* HTSGET_TICKET_SERVER_ADDR: The socket address for the server which creates response tickets. Default: "127.0.0.1:8080".
* HTSGET_TICKET_SERVER_ALLOW_CREDENTIALS: Boolean flag, indicating whether authenticated requests are allowed by including the `Access-Control-Allow-Credentials` header. Default: "false".
* HTSGET_TICKET_SERVER_ALLOW_ORIGIN: Which origin os allowed in the `ORIGIN` header. Default: "http://localhost:8080".

The following options are used for the data server.
* HTSGET_DATA_SERVER_ADDR: The socket address to use for the server which responds to tickets. Default: "127.0.0.1:8081". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_DATA_SERVER_KEY: The path to the PEM formatted X.509 private key used by the data server. Default: "None". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_DATA_SERVER_CERT: The path to the PEM formatted X.509 certificate used by the data server. Default: "None". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_DATA_SERVER_ALLOW_CREDENTIALS: Boolean flag, indicating whether authenticated requests are allowed by including the `Access-Control-Allow-Credentials` header. Default: "false"
* HTSGET_DATA_SERVER_ALLOW_ORIGIN: Which origin os allowed in the `ORIGIN` header. Default: "http://localhost:8081"

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

const ENVIRONMENT_VARIABLE_PREFIX: &str = "HTSGET";

fn default_localstorage_addr() -> SocketAddr {
  "127.0.0.1:8081".parse().expect("expected valid address")
}

fn default_addr() -> SocketAddr {
  "127.0.0.1:8080".parse().expect("expected valid address")
}

fn default_ticket_server_origin() -> String {
  "http://localhost:8080".to_string()
}

fn default_data_server_origin() -> String {
  "http://localhost:8081".to_string()
}

fn default_path() -> PathBuf {
  PathBuf::from("data")
}

fn default_serve_at() -> PathBuf {
  PathBuf::from("/data")
}

/// The command line arguments allowed for the htsget-rs executables.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = USAGE)]
pub struct Args {
  #[arg(short, long, env = "HTSGET_CONFIG")]
  config: PathBuf,
}

/// Specify the storage type to use.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StorageType {
  LocalStorage,
  #[cfg(feature = "s3-storage")]
  AwsS3Storage,
}

/// Configuration for the server. Each field will be read from environment variables.
#[derive(Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Config {
  #[serde(flatten)]
  pub resolver: RegexResolver,
  pub path: PathBuf,
  #[serde(flatten)]
  pub ticket_server_config: TicketServerConfig,
  pub storage_type: StorageType,
  #[serde(flatten)]
  pub data_server_config: DataServerConfig,
  #[cfg(feature = "s3-storage")]
  pub s3_bucket: String,
}

/// Configuration for the htsget server.
#[derive(Deserialize, Debug, Clone)]
#[serde(default)]
pub struct TicketServerConfig {
  pub ticket_server_addr: SocketAddr,
  #[serde(flatten)]
  pub service_info: ServiceInfo,
  pub ticket_server_cors_allow_credentials: bool,
  pub ticket_server_cors_allow_origin: String,
}

/// Configuration for the htsget server.
#[derive(Deserialize, Debug, Clone)]
#[serde(default)]
pub struct LocalDataServer {
  pub path: PathBuf,
  pub serve_at: PathBuf,
  pub addr: SocketAddr,
  pub key: Option<PathBuf>,
  pub cert: Option<PathBuf>,
  pub cors_allow_credentials: bool,
  pub cors_allow_origin: String,
}

impl Default for LocalDataServer {
  fn default() -> Self {
    Self {
      path: default_path(),
      serve_at: default_serve_at(),
      addr: default_localstorage_addr(),
      key: None,
      cert: None,
      cors_allow_credentials: false,
      cors_allow_origin: default_data_server_origin(),
    }
  }
}

/// Specify the storage type to use.
#[derive(Deserialize, Debug, Clone)]
#[non_exhaustive]
pub enum StorageTypeServer {
  LocalStorage(LocalDataServer),
  #[cfg(feature = "s3-storage")]
  AwsS3Storage(AwsS3DataServer),
}

impl Default for StorageTypeServer {
  fn default() -> Self {
    Self::LocalStorage(LocalDataServer::default())
  }
}

/// Configuration for the htsget server.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct AwsS3DataServer {
  pub bucket: String,
}

/// Configuration for the htsget server.
#[derive(Deserialize, Debug, Clone)]
#[serde(default)]
pub struct DataServerConfig {
  pub data_server_addr: SocketAddr,
  pub data_server_key: Option<PathBuf>,
  pub data_server_cert: Option<PathBuf>,
  pub data_server_cors_allow_credentials: bool,
  pub data_server_cors_allow_origin: String,
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

impl Default for TicketServerConfig {
  fn default() -> Self {
    Self {
      ticket_server_addr: default_addr(),
      service_info: ServiceInfo::default(),
      ticket_server_cors_allow_credentials: false,
      ticket_server_cors_allow_origin: default_ticket_server_origin(),
    }
  }
}

impl Default for DataServerConfig {
  fn default() -> Self {
    Self {
      data_server_addr: default_localstorage_addr(),
      data_server_key: None,
      data_server_cert: None,
      data_server_cors_allow_credentials: false,
      data_server_cors_allow_origin: default_data_server_origin(),
    }
  }
}

impl Default for Config {
  fn default() -> Self {
    Self {
      resolver: RegexResolver::default(),
      path: default_path(),
      ticket_server_config: Default::default(),
      storage_type: LocalStorage,
      data_server_config: Default::default(),
      #[cfg(feature = "s3-storage")]
      s3_bucket: "".to_string(),
    }
  }
}

impl Config {
  /// Parse the command line arguments
  pub fn parse_args() -> PathBuf {
    Args::parse().config
  }

  /// Read the environment variables into a Config struct.
  #[instrument]
  pub fn from_env(config: PathBuf) -> io::Result<Self> {
    let config = config::Config::builder()
      .add_source(File::from(config))
      .add_source(config::Environment::with_prefix(
        ENVIRONMENT_VARIABLE_PREFIX,
      ))
      .build()
      .map_err(|err| {
        io::Error::new(
          ErrorKind::Other,
          format!("config not properly set: {}", err),
        )
      })?
      .try_deserialize::<Config>()
      .map_err(|err| io::Error::new(ErrorKind::Other, format!("failed to parse config: {}", err)));

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
    std::env::set_var("HTSGET_TICKET_SERVER_ADDR", "127.0.0.1:8081");
    let config = Config::from_env(PathBuf::from("config.toml")).unwrap();
    assert_eq!(
      config.ticket_server_config.ticket_server_addr,
      "127.0.0.1:8081".parse().unwrap()
    );
  }

  #[test]
  fn config_ticket_server_cors_allow_origin() {
    std::env::set_var(
      "HTSGET_TICKET_SERVER_CORS_ALLOW_ORIGIN",
      "http://localhost:8080",
    );
    let config = Config::from_env(PathBuf::from("config.toml")).unwrap();
    assert_eq!(
      config.ticket_server_config.ticket_server_cors_allow_origin,
      "http://localhost:8080"
    );
  }

  #[test]
  fn config_data_server_cors_allow_origin() {
    std::env::set_var(
      "HTSGET_DATA_SERVER_CORS_ALLOW_ORIGIN",
      "http://localhost:8080",
    );
    let config = Config::from_env(PathBuf::from("config.toml")).unwrap();
    assert_eq!(
      config.data_server_config.data_server_cors_allow_origin,
      "http://localhost:8080"
    );
  }

  #[test]
  fn config_ticket_server_addr() {
    std::env::set_var("HTSGET_DATA_SERVER_ADDR", "127.0.0.1:8082");
    let config = Config::from_env(PathBuf::from("config.toml")).unwrap();
    assert_eq!(
      config.data_server_config.data_server_addr,
      "127.0.0.1:8082".parse().unwrap()
    );
  }

  #[test]
  fn config_regex() {
    std::env::set_var("HTSGET_REGEX", ".+");
    let config = Config::from_env(PathBuf::from("config.toml")).unwrap();
    assert_eq!(config.resolver.regex.to_string(), ".+");
  }

  #[test]
  fn config_substitution_string() {
    std::env::set_var("HTSGET_SUBSTITUTION_STRING", "$0-test");
    let config = Config::from_env(PathBuf::from("config.toml")).unwrap();
    assert_eq!(config.resolver.substitution_string, "$0-test");
  }

  #[test]
  fn config_service_info_id() {
    std::env::set_var("HTSGET_ID", "id");
    let config = Config::from_env(PathBuf::from("config.toml")).unwrap();
    assert_eq!(config.ticket_server_config.service_info.id.unwrap(), "id");
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn config_storage_type() {
    std::env::set_var("HTSGET_STORAGE_TYPE", "AwsS3Storage");
    let config = Config::from_env(PathBuf::from("config.toml")).unwrap();
    assert_eq!(config.storage_type, StorageType::AwsS3Storage);
  }
}
