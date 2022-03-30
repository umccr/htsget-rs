use std::path::PathBuf;

use serde::Deserialize;

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
pub struct HtsgetConfig {
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
