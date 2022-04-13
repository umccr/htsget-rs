use std::path::PathBuf;

use serde::Deserialize;

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

fn default_port() -> String {
  "8080".to_string()
}

fn default_ip() -> String {
  "127.0.0.1".to_string()
}

fn default_path() -> PathBuf {
  PathBuf::from(".")
}

fn default_regex_match() -> String {
  ".*".to_string()
}

fn default_regex_substitution() -> String {
  "$0".to_string()
}

/// Configuration for the server. Each field will be read from environment variables
#[derive(Deserialize, Debug, Clone)]
pub struct Config {
  #[serde(default = "default_port")]
  pub htsget_port: String,
  #[serde(default = "default_ip")]
  pub htsget_ip: String,
  #[serde(default = "default_path")]
  pub htsget_path: PathBuf,
  #[serde(default = "default_regex_match")]
  pub htsget_regex_match: String,
  #[serde(default = "default_regex_substitution")]
  pub htsget_regex_substitution: String,
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
  pub htsget_s3_bucket: Option<String>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      htsget_port: default_port(),
      htsget_ip: default_ip(),
      htsget_path: default_path(),
      htsget_regex_match: default_regex_match(),
      htsget_regex_substitution: default_regex_substitution(),
      htsget_id: None,
      htsget_name: None,
      htsget_version: None,
      htsget_organization_name: None,
      htsget_organization_url: None,
      htsget_contact_url: None,
      htsget_documentation_url: None,
      htsget_created_at: None,
      htsget_updated_at: None,
      htsget_environment: None,
      htsget_s3_bucket: None,
    }
  }
}
