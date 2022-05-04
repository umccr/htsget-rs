use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::PathBuf;

use crate::config::StorageType::LocalStorage;
use crate::regex_resolver::RegexResolver;
use serde::Deserialize;

pub const USAGE: &str = r#"
This executable doesn't use command line arguments, but there are some environment variables that can be set to configure the HtsGet server:
* HTSGET_ADDR: The socket address to use for the server which creates response tickets. Default: "127.0.0.1:8080".
* HTSGET_PATH: The path to the directory where the server should be started. Default: "."
* HTSGET_REGEX: The regular expression that should match an ID. Default: ".*".
* HTSGET_SUBSTITUTION_STRING: The replacement expression. Default: "$0".
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
* HTSGET_STORAGE_TYPE: Either LocalStorage or AwsS3Storage. Default: "LocalStorage".
* HTSGET_TICKET_SERVER_ADDR: The socket address to use for the server which responds to tickets. Default: "127.0.0.1:8081". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_TICKET_SERVER_KEY: The path to the PEM formatted X.509 private key used by the ticket response server. Default: "${HTSGET_PATH}/key.pem". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_TICKET_SERVER_CERT: The path to the PEM formatted X.509 certificate used by the ticket response server. Default: "${HTSGET_PATH}/cert.pem". Unused if HTSGET_STORAGE_TYPE is not "LocalStorage".
* HTSGET_S3_BUCKET: The name of the AWS S3 bucket. Default: None. Unused if HTSGET_STORAGE_TYPE is not "AwsS3Storage". Must be specified if using AwsS3Storage.
"#;

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
  default_path().join("cert.pem")
}

fn default_localstorage_key() -> PathBuf {
  default_path().join("key.pem")
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
  pub htsget_addr: SocketAddr,
  #[serde(flatten)]
  pub htsget_resolver: RegexResolver,
  pub htsget_path: PathBuf,
  #[serde(flatten)]
  pub htsget_config_service_info: ConfigServiceInfo,
  pub htsget_storage_type: StorageType,
  pub htsget_localstorage_addr: SocketAddr,
  pub htsget_localstorage_key: PathBuf,
  pub htsget_localstorage_cert: PathBuf,
  #[cfg(feature = "s3-storage")]
  pub htsget_s3_bucket: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct ConfigServiceInfo {
  pub htsget_id: Option<String>,
  pub htsget_name: Option<String>,
  pub htsget_version: Option<String>,
  pub htsget_organization_name: Option<String>,
  pub htsget_organization_url: Option<String>,
  pub htsget_contact_url: Option<String>,
  pub htsget_documentation_url: Option<String>,
  pub htsget_created_at: Option<String>,
  pub htsget_updated_at: Option<String>,
  pub htsget_environment: Option<String>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      htsget_addr: default_addr(),
      htsget_resolver: RegexResolver::default(),
      htsget_path: default_path(),
      htsget_config_service_info: ConfigServiceInfo::default(),
      htsget_storage_type: LocalStorage,
      htsget_localstorage_addr: default_localstorage_addr(),
      htsget_localstorage_key: default_localstorage_key(),
      htsget_localstorage_cert: default_localstorage_cert(),
      #[cfg(feature = "s3-storage")]
      htsget_s3_bucket: None,
    }
  }
}

impl Config {
  pub fn from_env() -> std::io::Result<Self> {
    envy::from_env().map_err(|err| {
      std::io::Error::new(
        ErrorKind::Other,
        format!("Config not properly set: {}", err),
      )
    })
  }
}

mod tests {
  use crate::config::Config;
  use crate::config::StorageType::AwsS3Storage;


  #[test]
  fn config_addr() {
    std::env::set_var("HTSGET_ADDR", "127.0.0.1:8081");
    let config = Config::from_env().unwrap();
    assert_eq!(config.htsget_addr, "127.0.0.1:8081".parse().unwrap());
  }

  #[test]
  fn config_regex() {
    std::env::set_var("HTSGET_REGEX", ".+");
    let config = Config::from_env().unwrap();
    assert_eq!(config.htsget_resolver.htsget_regex.to_string(), ".+");
  }

  #[test]
  fn config_substitution_string() {
    std::env::set_var("HTSGET_SUBSTITUTION_STRING", "$0-test");
    let config = Config::from_env().unwrap();
    assert_eq!(config.htsget_resolver.htsget_substitution_string, "$0-test");
  }

  #[test]
  fn config_service_info_id() {
    std::env::set_var("HTSGET_ID", "id");
    let config = Config::from_env().unwrap();
    assert_eq!(config.htsget_config_service_info.htsget_id.unwrap(), "id");
  }

  #[test]
  fn config_storage_type() {
    std::env::set_var("HTSGET_STORAGE_TYPE", "AwsS3Storage");
    let config = Config::from_env().unwrap();
    assert_eq!(config.htsget_storage_type, AwsS3Storage);
  }
}
