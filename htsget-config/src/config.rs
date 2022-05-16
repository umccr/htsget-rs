use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::PathBuf;

use serde::Deserialize;
use tracing::debug;

use crate::config::StorageType::LocalStorage;
use crate::regex_resolver::RegexResolver;

pub const USAGE: &str = r#"
This executable doesn't use command line arguments, but there are some environment variables that can be set to configure the HtsGet server:
* HTSGET_ADDR: The socket address to use for the server which creates response tickets. Default: "127.0.0.1:8080".
* HTSGET_PATH: The path to the directory where the server should be started. Default: "."
* HTSGET_REGEX: The regular expression that should match an ID. Default: ".*".
* HTSGET_SUBSTITUTION_STRING: The replacement expression. Default: "$0".
* HTSGET_STORAGE_TYPE: Either LocalStorage or AwsS3Storage. Default: "LocalStorage".
* HTSGET_TICKET_SERVER_ADDR: The socket address to use for the server which responds to tickets. Default: "127.0.0.1:8081". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_TICKET_SERVER_KEY: The path to the PEM formatted X.509 private key used by the ticket response server. Default: "key.pem". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_TICKET_SERVER_CERT: The path to the PEM formatted X.509 certificate used by the ticket response server. Default: "cert.pem". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_S3_BUCKET: The name of the AWS S3 bucket. Default: "". Unused if HTSGET_STORAGE_TYPE is not "AwsS3Storage".
For more information about the regex options look in the documentation of the regex crate(https://docs.rs/regex/).
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
  "127.0.0.1:8081".parse().expect("Expected valid address.")
}

fn default_addr() -> SocketAddr {
  "127.0.0.1:8080".parse().expect("Expected valid address.")
}

fn default_path() -> PathBuf {
  PathBuf::from(".")
}

fn default_localstorage_cert() -> PathBuf {
  PathBuf::from("cert.pem")
}

fn default_localstorage_key() -> PathBuf {
  PathBuf::from("key.pem")
}

/// Specify the storage type to use.
#[derive(Deserialize, Debug, Clone, PartialEq)]
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
  pub service_info: ConfigServiceInfo,
  pub storage_type: StorageType,
  pub ticket_server_addr: SocketAddr,
  pub ticket_server_key: PathBuf,
  pub ticket_server_cert: PathBuf,
  #[cfg(feature = "s3-storage")]
  pub s3_bucket: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct ConfigServiceInfo {
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
      service_info: ConfigServiceInfo::default(),
      storage_type: LocalStorage,
      ticket_server_addr: default_localstorage_addr(),
      ticket_server_key: default_localstorage_key(),
      ticket_server_cert: default_localstorage_cert(),
      #[cfg(feature = "s3-storage")]
      s3_bucket: "".to_string(),
    }
  }
}

impl Config {
  /// Read the environment variables into a Config struct.
  pub fn from_env() -> std::io::Result<Self> {
    let config = envy::prefixed(ENVIRONMENT_VARIABLE_PREFIX)
      .from_env()
      .map_err(|err| {
        std::io::Error::new(
          ErrorKind::Other,
          format!("Config not properly set: {}", err),
        )
      });
    debug!(config = ?config, "Config created from environment variables.");
    config
  }
}

#[cfg(test)]
mod tests {
  use crate::config::StorageType::AwsS3Storage;

  use super::*;

  #[test]
  fn config_addr() {
    std::env::set_var("HTSGET_ADDR", "127.0.0.1:8081");
    let config = Config::from_env().unwrap();
    assert_eq!(config.addr, "127.0.0.1:8081".parse().unwrap());
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

  #[test]
  fn config_storage_type() {
    std::env::set_var("HTSGET_STORAGE_TYPE", "AwsS3Storage");
    let config = Config::from_env().unwrap();
    assert_eq!(config.storage_type, AwsS3Storage);
  }
}
