use serde::Deserialize;
use std::path::PathBuf;

fn default_port() -> String {
  "8080".to_string()
}

fn default_ip() -> String {
  "127.0.0.1".to_string()
}

fn default_path() -> PathBuf {
  PathBuf::from(".")
}

#[derive(Deserialize, Debug)]
pub struct Config {
  #[serde(default = "default_port")]
  pub htsget_port: String,
  #[serde(default = "default_ip")]
  pub htsget_ip: String,
  #[serde(default = "default_path")]
  pub htsget_path: PathBuf,
}
