use std::fs;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use async_trait::async_trait;
use htsget_config::config::cors::{AllowType, CorsConfig};
use htsget_config::config::{CertificateKeyPair, DataServerConfig, TicketServerConfig};
use htsget_config::regex_resolver::{LocalResolver, RegexResolver, Scheme, StorageType};
use htsget_config::TaggedTypeAll;
use http::uri::Authority;
use http::HeaderMap;
use serde::de;

use crate::util::generate_test_certificates;
use crate::Config;

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

/// Get the default test resolver.
pub fn default_test_resolver(addr: SocketAddr, scheme: Scheme) -> RegexResolver {
  let resolver = LocalResolver::new(
    scheme,
    Authority::from_str(&addr.to_string()).unwrap(),
    default_dir_data().to_str().unwrap().to_string(),
    "/data".to_string(),
  );

  RegexResolver::new(StorageType::Local(resolver), ".*", "$0", Default::default()).unwrap()
}

/// Default config with fixed port.
pub fn default_config_fixed_port() -> Config {
  let addr = "127.0.0.1:8081".parse().unwrap();

  default_test_config_params(addr, None, Scheme::Http)
}

fn get_dynamic_addr() -> SocketAddr {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  listener.local_addr().unwrap()
}

/// Set the default cors testing config.
pub fn default_cors_config() -> CorsConfig {
  CorsConfig::new(
    false,
    AllowType::List(vec!["http://example.com".parse().unwrap()]),
    AllowType::Tagged(TaggedTypeAll::All),
    AllowType::Tagged(TaggedTypeAll::All),
    1000,
    AllowType::List(vec![]),
  )
}

fn default_test_config_params(
  addr: SocketAddr,
  tls: Option<CertificateKeyPair>,
  scheme: Scheme,
) -> Config {
  let cors = default_cors_config();
  let server_config = DataServerConfig::new(
    true,
    addr,
    default_dir_data(),
    PathBuf::from("/data"),
    tls,
    cors.clone(),
  );

  Config::new(
    TicketServerConfig::new("127.0.0.1:8080".parse().unwrap(), cors, Default::default()),
    server_config,
    vec![default_test_resolver(addr, scheme)],
  )
}

/// Default config using the current cargo manifest directory, and dynamic port.
pub fn default_test_config() -> Config {
  let addr = get_dynamic_addr();

  default_test_config_params(addr, None, Scheme::Http)
}

/// Config with tls ticket server, using the current cargo manifest directory.
pub fn config_with_tls<P: AsRef<Path>>(path: P) -> Config {
  let addr = get_dynamic_addr();
  let (key_path, cert_path) = generate_test_certificates(path, "key.pem", "cert.pem");

  default_test_config_params(
    addr,
    Some(CertificateKeyPair::new(cert_path, key_path)),
    Scheme::Https,
  )
}

/// Get the event associated with the file.
pub fn get_test_file<P: AsRef<Path>>(path: P) -> String {
  let path = default_dir().join("data").join(path);
  fs::read_to_string(path).expect("failed to read file")
}
