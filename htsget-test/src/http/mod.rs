//! Testing functionality related to http and url tickets.
//!

pub mod concat;
pub mod cors;
pub mod server;

use std::fs;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use async_trait::async_trait;
use htsget_config::config::advanced::cors::{AllowType, CorsConfig, TaggedAllowTypes};
use htsget_config::config::advanced::regex_location::RegexLocation;
use htsget_config::config::data_server::{DataServerConfig, DataServerEnabled};
use htsget_config::config::location::{LocationEither, Locations};
use htsget_config::config::ticket_server::TicketServerConfig;
use htsget_config::config::Config;
use htsget_config::storage::file::File;
use htsget_config::storage::Backend;
use htsget_config::tls::{
  load_certs, load_key, tls_server_config, CertificateKeyPair, TlsServerConfig,
};
use htsget_config::types::Scheme;
use http::uri::Authority;
use http::{HeaderMap, HeaderName, Method};
use serde::de;

use crate::util::{default_dir, default_dir_data, generate_test_certificates};

/// Represents a http header.
#[derive(Debug)]
pub struct Header<K, V> {
  pub name: K,
  pub value: V,
}

impl<K: Into<HeaderName>, V: Into<http::HeaderValue>> Header<K, V> {
  pub fn into_tuple(self) -> (HeaderName, http::HeaderValue) {
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
  fn insert_header(
    self,
    header: Header<impl Into<HeaderName>, impl Into<http::HeaderValue>>,
  ) -> Self;
  fn set_payload(self, payload: impl Into<String>) -> Self;
  fn uri(self, uri: impl Into<String>) -> Self;
  fn method(self, method: impl Into<Method>) -> Self;
}

/// Mock server trait that should be implemented to use test functions.
#[async_trait(?Send)]
pub trait TestServer<T: TestRequest> {
  async fn get_expected_path(&self) -> String;
  fn get_config(&self) -> &Config;
  fn request(&self) -> T;
  async fn test_server(&self, request: T, expected_path: String) -> Response;
}

/// Get the default test storage.
pub fn default_test_resolver(addr: SocketAddr, scheme: Scheme) -> Locations {
  let local_storage = File::new(
    scheme,
    Authority::from_str(&addr.to_string()).unwrap(),
    default_dir_data().to_str().unwrap().to_string(),
  );

  Locations::new(vec![
    LocationEither::Regex(
      RegexLocation::new(
        "^1-(.*)$".parse().unwrap(),
        "$1".to_string(),
        Backend::File(local_storage.clone()),
        Default::default(),
      )
      .into(),
    ),
    LocationEither::Regex(
      RegexLocation::new(
        "^2-(.*)$".parse().unwrap(),
        "$1".to_string(),
        Backend::File(local_storage.clone()),
        Default::default(),
      )
      .into(),
    ),
  ])
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
    AllowType::Tagged(TaggedAllowTypes::All),
    AllowType::Tagged(TaggedAllowTypes::All),
    1000,
    AllowType::List(vec![]),
  )
}

fn default_test_config_params(
  addr: SocketAddr,
  tls: Option<TlsServerConfig>,
  scheme: Scheme,
) -> Config {
  let cors = default_cors_config();
  let server_config = DataServerConfig::new(addr, default_dir_data(), tls.clone(), cors.clone());

  Config::new(
    Default::default(),
    TicketServerConfig::new("127.0.0.1:8080".parse().unwrap(), tls, cors),
    DataServerEnabled::Some(server_config),
    Default::default(),
    default_test_resolver(addr, scheme),
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
    Some(test_tls_server_config(key_path, cert_path)),
    Scheme::Https,
  )
}

/// Get a test tls server config.
pub fn test_tls_server_config(key_path: PathBuf, cert_path: PathBuf) -> TlsServerConfig {
  let key = load_key(key_path).unwrap();
  let certs = load_certs(cert_path).unwrap();
  let server_config = tls_server_config(CertificateKeyPair::new(certs, key)).unwrap();

  TlsServerConfig::new(server_config)
}

/// Get the event associated with the file.
pub fn get_test_file<P: AsRef<Path>>(path: P) -> String {
  let path = default_dir().join("data").join(path);
  fs::read_to_string(path).expect("failed to read file")
}
