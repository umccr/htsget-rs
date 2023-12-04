use std::fs::File as StdFile;
use std::io::{Cursor, Read};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[cfg(feature = "crypt4gh")]
use async_crypt4gh::reader::builder::Builder;
#[cfg(feature = "crypt4gh")]
use async_crypt4gh::SenderPublicKey;
use async_trait::async_trait;
use base64::engine::general_purpose;
use base64::Engine;
use http::uri::Authority;
use http::HeaderMap;
use serde::de;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use htsget_config::config::cors::{AllowType, CorsConfig};
use htsget_config::config::{DataServerConfig, TicketServerConfig};
use htsget_config::resolver::Resolver;
use htsget_config::storage::{local::LocalStorage, Storage};
use htsget_config::tls::{
  load_certs, load_key, tls_server_config, CertificateKeyPair, TlsServerConfig,
};
use htsget_config::types;
use htsget_config::types::{Scheme, TaggedTypeAll};

#[cfg(feature = "crypt4gh")]
use crate::crypt4gh::get_keys;
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
  async fn get_expected_path(&self) -> String;
  fn get_config(&self) -> &Config;
  fn get_request(&self) -> T;
  async fn test_server(&self, request: T, expected_path: String) -> Response;
}

/// Get the default directory.
pub fn default_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .unwrap()
    .to_path_buf()
}

/// Test a response against a bam file including the sliced byte ranges.
pub async fn test_bam_file_byte_ranges(response: types::Response, file: PathBuf) {
  let file_str = file.to_str().unwrap();
  let mut buf = vec![];
  StdFile::open(file_str)
    .unwrap()
    .read_to_end(&mut buf)
    .unwrap();

  let output = response
    .urls
    .into_iter()
    .map(|url| {
      if let Some(data_uri) = url.url.as_str().strip_prefix("data:;base64,") {
        general_purpose::STANDARD.decode(data_uri).unwrap()
      } else {
        let headers = url.headers.unwrap().into_inner();
        let range = headers.get("Range").unwrap();
        let mut headers = range.strip_prefix("bytes=").unwrap().split('-');

        let start = usize::from_str(headers.next().unwrap()).unwrap();
        let end = usize::from_str(headers.next().unwrap()).unwrap() + 1;

        buf[start..end].to_vec()
      }
    })
    .reduce(|acc, x| [acc, x].concat())
    .unwrap();

  #[cfg(feature = "crypt4gh")]
  if file_str.ends_with(".c4gh") {
    let (recipient_private_key, sender_public_key) = get_keys().await;

    let mut reader = Builder::default()
      .with_sender_pubkey(SenderPublicKey::new(sender_public_key))
      .build_with_stream_length(Cursor::new(output), vec![recipient_private_key])
      .await
      .unwrap();

    let mut unencrypted_out = vec![];
    reader.read_to_end(&mut unencrypted_out).await.unwrap();
  }

  // Todo investigate why noodles fails here but samtools doesn't.
  // let mut reader = bam::AsyncReader::new(bgzf::AsyncReader::new(output.as_slice()));
  // let header = reader.read_header().await.unwrap().parse().unwrap();
  // reader.read_reference_sequences().await.unwrap();
  // println!("{header}");
  //
  // let mut records = reader.records(&header);
  // while let Some(record) = records.try_next().await.unwrap() {
  //   println!("{:#?}", record);
  //   continue;
  // }
}

/// Get the default directory where data is present..
pub fn default_dir_data() -> PathBuf {
  default_dir().join("data")
}

/// Get the default test storage.
pub fn default_test_resolver(addr: SocketAddr, scheme: Scheme) -> Vec<Resolver> {
  let local_storage = LocalStorage::new(
    scheme,
    Authority::from_str(&addr.to_string()).unwrap(),
    default_dir_data().to_str().unwrap().to_string(),
    "/data".to_string(),
  );
  vec![
    Resolver::new(
      Storage::Local {
        local_storage: local_storage.clone(),
      },
      "^1-(.*)$",
      "$1",
      Default::default(),
      Default::default(),
    )
    .unwrap(),
    Resolver::new(
      Storage::Local { local_storage },
      "^2-(.*)$",
      "$1",
      Default::default(),
      Default::default(),
    )
    .unwrap(),
  ]
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
  tls: Option<TlsServerConfig>,
  scheme: Scheme,
) -> Config {
  let cors = default_cors_config();
  let server_config = DataServerConfig::new(
    true,
    addr,
    default_dir_data(),
    "/data".to_string(),
    tls.clone(),
    cors.clone(),
  );

  Config::new(
    Default::default(),
    TicketServerConfig::new("127.0.0.1:8080".parse().unwrap(), tls, cors),
    server_config,
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

/// Get a test file as a string.
pub async fn get_test_file_string<P: AsRef<Path>>(path: P) -> String {
  let mut string = String::new();
  get_test_file(path)
    .await
    .read_to_string(&mut string)
    .await
    .expect("failed to read to string");
  string
}

/// Get a test file path.
pub fn get_test_path<P: AsRef<Path>>(path: P) -> PathBuf {
  default_dir().join("data").join(path)
}

/// Get a test file.
pub async fn get_test_file<P: AsRef<Path>>(path: P) -> File {
  File::open(get_test_path(path))
    .await
    .expect("failed to read file")
}
