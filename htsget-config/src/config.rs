use std::net::SocketAddr;
use std::path::PathBuf;

use serde::Deserialize;
use crate::config::StorageType::LocalStorage;
use crate::regex_resolver::RegexResolver;

pub const USAGE: &str = r#"
This executable doesn't use command line arguments, but there are some environment variables that can be set to configure the HtsGet server:
* HTSGET_IP: The ip to use. Default: 127.0.0.1
* HTSGET_PORT: The port to use. Default: 8080
* HTSGET_PATH: The path to the directory where the server should be started. Default: Actual directory
* HTSGET_REGEX: The regular expression that should match an ID. Default: ".*"
* HTSGET_REPLACEMENT: The replacement expression. Default: "$0"
For more information about the regex options look in the documentation of the regex crate(https://docs.rs/regex/).
The next variables are used to configure the info for the service-info endpoints
* HTSGET_ID: The id of the service. Default: ""
* HTSGET_NAME: The name of the service. Default: "HtsGet service"
* HTSGET_VERSION: The version of the service. Default: ""
* HTSGET_ORGANIZATION_NAME: The name of the organization. Default: "Snake oil"
* HTSGET_ORGANIZATION_URL: The url of the organization. Default: "https://en.wikipedia.org/wiki/Snake_oil"
* HTSGET_CONTACT_URL: A url to provide contact to the users. Default: "",
* HTSGET_DOCUMENTATION_URL: A link to the documentation. Default: "https://github.com/umccr/htsget-rs/tree/main/htsget-http-actix",
* HTSGET_CREATED_AT: Date of the creation of the service. Default: "",
* HTSGET_UPDATED_AT: Date of the last update of the service. Default: "",
* HTSGET_ENVIRONMENT: The environment in which the service is running. Default: "Testing",
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

fn default_resolver() -> RegexResolver {
  RegexResolver::new(".*", "$0").expect("Expected valid resolver.")
}

fn default_localstorage_cert() -> PathBuf {
  default_path().join("certs/cert.pem")
}

fn default_localstorage_key() -> PathBuf {
  default_path().join("certs/key.pem")
}

/// Specify the storage type to use.
#[derive(Deserialize, Debug, Clone)]
pub enum StorageType {
  LocalStorage,
  #[cfg(feature = "s3-storage")]
  AwsS3Storage
}

/// Configuration for the server. Each field will be read from environment variables
#[derive(Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Config {
  pub htsget_addr: SocketAddr,
  pub htsget_resolver: RegexResolver,
  pub htsget_path: PathBuf,
  #[serde(flatten)]
  pub htsget_config_service_info: ConfigServiceInfo,
  pub htsget_localstorage_addr: SocketAddr,
  pub htsget_localstorage_cert: PathBuf,
  pub htsget_localstorage_key: PathBuf,
  pub htsget_storage_type: StorageType,
  #[cfg(feature = "s3-storage")]
  pub htsget_s3_bucket: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
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
      htsget_resolver: default_resolver(),
      htsget_path: default_path(),
      htsget_config_service_info: ConfigServiceInfo::default(),
      htsget_localstorage_addr: default_localstorage_addr(),
      htsget_localstorage_cert: default_localstorage_cert(),
      htsget_localstorage_key: default_localstorage_key(),
      htsget_storage_type: LocalStorage,
      #[cfg(feature = "s3-storage")]
      htsget_s3_bucket: None,
    }
  }
}
