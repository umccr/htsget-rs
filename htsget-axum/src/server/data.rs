//! The axum data server.
//!

use crate::error::Result;
use crate::server::{configure_cors, BindServer, Server};
use axum::Router;
use htsget_config::config::advanced::cors::CorsConfig;
use htsget_config::config::data_server::DataServerConfig;
use std::net::SocketAddr;
use std::path::Path;
use tokio::task::JoinHandle;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::info;

/// An data block server.
#[derive(Debug)]
pub struct DataServer {
  server: Server,
  cors: CorsConfig,
}

impl DataServer {
  /// Create a new data server.
  pub fn new(server: Server, cors: CorsConfig) -> Self {
    Self { server, cors }
  }

  /// Run the data server, using the provided path, key and certificate.
  pub async fn serve<P: AsRef<Path>>(self, path: P) -> Result<()> {
    self.server.serve(Self::router(self.cors, path)).await
  }

  /// Create the router for the data server.
  pub fn router<P: AsRef<Path>>(cors: CorsConfig, path: P) -> Router {
    Router::new()
      .nest_service("/", ServeDir::new(path))
      .layer(configure_cors(cors))
      .layer(TraceLayer::new_for_http())
  }

  /// Get the local address the server has bound to.
  pub fn local_addr(&self) -> Result<SocketAddr> {
    self.server.local_addr()
  }
}

impl From<DataServerConfig> for BindServer {
  /// Returns a data server with TLS enabled if the tls config is not None or without TLS enabled
  /// if it is None.
  fn from(config: DataServerConfig) -> Self {
    let addr = config.addr();
    let cors = config.cors().clone();

    match config.into_tls() {
      None => Self::new(addr, cors),
      Some(tls) => Self::new_with_tls(addr, cors, tls),
    }
  }
}

/// Spawn a task to run the data server.
pub async fn join_handle(config: DataServerConfig) -> Result<JoinHandle<Result<()>>> {
  let local_path = config.local_path().to_path_buf();
  let data_server = BindServer::from(config.clone()).bind_data_server().await?;

  info!(address = ?data_server.local_addr()?, "data server address bound to");

  Ok(tokio::spawn(
    async move { data_server.serve(&local_path).await },
  ))
}

#[cfg(test)]
mod tests {
  use std::str::FromStr;

  use async_trait::async_trait;
  use http::header::HeaderName;
  use http::{HeaderMap, Method};
  use reqwest::{Client, ClientBuilder, RequestBuilder};
  use rustls::crypto::aws_lc_rs;
  use tempfile::{tempdir, TempDir};
  use tokio::fs::{create_dir, File};
  use tokio::io::AsyncWriteExt;

  use htsget_config::config::Config;
  use htsget_config::tls::TlsServerConfig;
  use htsget_config::types::Scheme;
  use htsget_test::http::cors::{test_cors_preflight_request_uri, test_cors_simple_request_uri};
  use htsget_test::http::{
    config_with_tls, default_cors_config, default_test_config, Header, Response as TestResponse,
    TestRequest, TestServer,
  };

  use super::*;

  struct DataTestServer {
    config: Config,
  }

  struct DataTestRequest {
    client: Client,
    headers: HeaderMap,
    payload: String,
    method: Method,
    uri: String,
  }

  impl TestRequest for DataTestRequest {
    fn insert_header(
      mut self,
      header: Header<impl Into<HeaderName>, impl Into<http::HeaderValue>>,
    ) -> Self {
      self.headers.insert(header.name.into(), header.value.into());
      self
    }

    fn set_payload(mut self, payload: impl Into<String>) -> Self {
      self.payload = payload.into();
      self
    }

    fn uri(mut self, uri: impl Into<String>) -> Self {
      self.uri = uri.into().parse().unwrap();
      self
    }

    fn method(mut self, method: impl Into<Method>) -> Self {
      self.method = method.into();
      self
    }
  }

  impl DataTestRequest {
    fn build(self) -> RequestBuilder {
      self
        .client
        .request(self.method, self.uri)
        .headers(self.headers)
        .body(self.payload)
    }
  }

  impl Default for DataTestRequest {
    fn default() -> Self {
      Self {
        client: ClientBuilder::new()
          .danger_accept_invalid_certs(true)
          .use_rustls_tls()
          .build()
          .unwrap(),
        headers: HeaderMap::default(),
        payload: "".to_string(),
        method: Method::GET,
        uri: "".to_string(),
      }
    }
  }

  impl Default for DataTestServer {
    fn default() -> Self {
      Self {
        config: default_test_config(),
      }
    }
  }

  #[async_trait(?Send)]
  impl TestServer<DataTestRequest> for DataTestServer {
    async fn get_expected_path(&self) -> String {
      "".to_string()
    }

    fn get_config(&self) -> &Config {
      &self.config
    }

    fn request(&self) -> DataTestRequest {
      DataTestRequest::default()
    }

    async fn test_server(&self, request: DataTestRequest, expected_path: String) -> TestResponse {
      let response = request.build().send().await.unwrap();
      let status: u16 = response.status().into();
      let headers = response.headers().clone();
      let bytes = response.bytes().await.unwrap().to_vec();

      TestResponse::new(status, headers, bytes, expected_path)
    }
  }

  #[tokio::test]
  async fn test_http_server() {
    let (_, base_path) = create_local_test_files().await;

    test_server("http", None, base_path.path().to_path_buf()).await;
  }

  #[tokio::test]
  async fn test_tls_server() {
    let _ = aws_lc_rs::default_provider().install_default();

    let (_, base_path) = create_local_test_files().await;
    let data_server = config_with_tls(base_path.path())
      .data_server()
      .as_data_server_config()
      .unwrap()
      .clone();
    let server_config = data_server.into_tls().unwrap();

    test_server("https", Some(server_config), base_path.path().to_path_buf()).await;
  }

  #[test]
  fn http_scheme() {
    let formatter = BindServer::new("127.0.0.1:8080".parse().unwrap(), CorsConfig::default());
    assert_eq!(formatter.get_scheme(), &Scheme::Http);
  }

  #[test]
  fn https_scheme() {
    assert_eq!(tls_formatter().get_scheme(), &Scheme::Https);
  }

  #[tokio::test]
  async fn get_addr_local_addr() {
    let mut formatter = BindServer::new("127.0.0.1:0".parse().unwrap(), CorsConfig::default());
    let server = formatter.bind_server().await.unwrap();
    assert_eq!(formatter.get_addr(), server.local_addr().unwrap());
  }

  #[tokio::test]
  async fn cors_simple_response() {
    let (_, base_path) = create_local_test_files().await;

    let port = start_data_server(None, base_path.path().to_path_buf()).await;

    test_cors_simple_request_uri(
      &DataTestServer::default(),
      &format!("http://localhost:{port}/key1"),
    )
    .await;
  }

  #[tokio::test]
  async fn cors_options_response() {
    let (_, base_path) = create_local_test_files().await;

    let port = start_data_server(None, base_path.path().to_path_buf()).await;

    test_cors_preflight_request_uri(
      &DataTestServer::default(),
      &format!("http://localhost:{port}/key1"),
    )
    .await;
  }

  fn tls_formatter() -> BindServer {
    let _ = aws_lc_rs::default_provider().install_default();

    let tmp_dir = tempdir().unwrap();
    let data_server = config_with_tls(tmp_dir.path())
      .data_server()
      .as_data_server_config()
      .unwrap()
      .clone();
    let server_config = data_server.clone().into_tls().unwrap();

    BindServer::new_with_tls(
      "127.0.0.1:8080".parse().unwrap(),
      CorsConfig::default(),
      server_config,
    )
  }

  async fn start_data_server<P>(cert_key_pair: Option<TlsServerConfig>, path: P) -> u16
  where
    P: AsRef<Path> + Send + 'static,
  {
    let addr = SocketAddr::from_str(&format!("{}:{}", "127.0.0.1", "0")).unwrap();
    let server = Server::bind_addr(addr, cert_key_pair).await.unwrap();
    let port = server.local_addr().unwrap().port();

    let data_server = DataServer::new(server, default_cors_config());
    tokio::spawn(async move { data_server.serve(path).await.unwrap() });

    port
  }

  async fn test_server<P>(scheme: &str, cert_key_pair: Option<TlsServerConfig>, path: P)
  where
    P: AsRef<Path> + Send + 'static,
  {
    let port = start_data_server(cert_key_pair, path).await;

    let test_server = DataTestServer::default();
    let request = test_server
      .request()
      .method(Method::GET)
      .uri(format!("{scheme}://localhost:{port}/key1"));
    let response = test_server.test_server(request, "".to_string()).await;

    assert!(response.is_success());
    assert_eq!(response.body, b"value1");
  }

  pub(crate) async fn create_local_test_files() -> (String, TempDir) {
    let base_path = TempDir::new().unwrap();

    let folder_name = "folder";
    let key1 = "key1";
    let value1 = b"value1";
    let key2 = "key2";
    let value2 = b"value2";
    File::create(base_path.path().join(key1))
      .await
      .unwrap()
      .write_all(value1)
      .await
      .unwrap();
    create_dir(base_path.path().join(folder_name))
      .await
      .unwrap();
    File::create(base_path.path().join(folder_name).join(key2))
      .await
      .unwrap()
      .write_all(value2)
      .await
      .unwrap();

    (folder_name.to_string(), base_path)
  }
}
