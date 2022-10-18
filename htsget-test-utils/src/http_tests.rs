use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use http::HeaderMap;
use serde::de;

use htsget_config::config::Config;
use htsget_search::storage::data_server::HttpTicketFormatter;

use crate::util::generate_test_certificates;

/// Represents a http header.
#[derive(Debug)]
pub struct Header<T: Into<String>> {
  pub name: T,
  pub value: T,
}

impl<T: Into<String>> Header<T> {
  pub fn into_tuple(self) -> (String, String) {
    (self.name.into(), self.value.into())
  }
}

/// Represents a http response.
#[derive(Debug)]
pub struct Response {
  pub status: u16,
  pub headers: HeaderMap,
  pub body: Vec<u8>,
  pub expected_url_path: String,
}

impl Response {
  pub fn new(status: u16, headers: HeaderMap, body: Vec<u8>, expected_url_path: String) -> Self {
    Self {
      status,
      headers,
      body,
      expected_url_path,
    }
  }

  /// Deserialize the body from a slice.
  pub fn deserialize_body<T>(&self) -> Result<T, serde_json::Error>
  where
    T: de::DeserializeOwned,
  {
    serde_json::from_slice(&self.body)
  }

  /// Check if status code is success.
  pub fn is_success(&self) -> bool {
    300 > self.status && self.status >= 200
  }
}

/// Mock request trait that should be implemented to use test functions.
pub trait TestRequest {
  fn insert_header(self, header: Header<impl Into<String>>) -> Self;
  fn set_payload(self, payload: impl Into<String>) -> Self;
  fn uri(self, uri: impl Into<String>) -> Self;
  fn method(self, method: impl Into<String>) -> Self;
}

/// Mock server trait that should be implemented to use test functions.
#[async_trait(?Send)]
pub trait TestServer<T: TestRequest> {
  fn get_config(&self) -> &Config;
  fn get_request(&self) -> T;
  async fn test_server(&self, request: T) -> Response;
}

/// Get the default directory.
pub fn default_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .unwrap()
    .to_path_buf()
}

/// Get the default directory where data is present..
pub fn default_dir_data() -> PathBuf {
  default_dir().join("data")
}

fn set_path(config: &mut Config) {
  config.path = default_dir_data();
}

fn set_addr_and_path(config: &mut Config) {
  set_path(config);
  config.data_server_config.data_server_addr = "127.0.0.1:0".parse().unwrap();
}

/// Get the [HttpTicketFormatter] from the config.
pub fn formatter_from_config(config: &Config) -> HttpTicketFormatter {
  HttpTicketFormatter::try_from(config.data_server_config.clone()).unwrap()
}

/// Default config with fixed port.
pub fn default_config_fixed_port() -> Config {
  let mut config = Config::default();
  set_path(&mut config);
  config
}

/// Default config using the current cargo manifest directory, and dynamic port.
pub fn default_test_config() -> Config {
  let mut config = Config::default();
  set_addr_and_path(&mut config);

  config.data_server_config.data_server_cors_allow_credentials = false;
  config.data_server_config.data_server_cors_allow_origin = "http://example.com".to_string();

  config
}

/// Config with tls ticket server, using the current cargo manifest directory.
pub fn config_with_tls<P: AsRef<Path>>(path: P) -> Config {
  let mut config = Config::default();
  set_addr_and_path(&mut config);

  let (key_path, cert_path) = generate_test_certificates(path, "key.pem", "cert.pem");
  config.data_server_config.data_server_key = Some(key_path);
  config.data_server_config.data_server_cert = Some(cert_path);

  config
}

/// Get the event associated with the file.
pub fn get_test_file<P: AsRef<Path>>(path: P) -> String {
  let path = default_dir().join("data").join(path);
  fs::read_to_string(path).expect("failed to read file")
}
