//! The following module provides an implementation of [UrlFormatter] for https, and the server
//! code which responds to formatted urls.
//!
//! This is the code that replies to the url tickets generated by [HtsGet], in the case of [LocalStorage].
//!

use std::fs::File;
use std::io::BufReader;
use std::net::{AddrParseError, SocketAddr};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use axum::http;
use axum::Router;
use axum_extra::routing::SpaRouter;
use futures_util::future::poll_fn;
use htsget_config::config::cors::CorsConfig;
use htsget_config::config::DataServerConfig;
use http::uri::Scheme;
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, Http};
use rustls_pemfile::{certs, pkcs8_private_keys};
use tokio::net::TcpListener;
use tokio_rustls::rustls::{Certificate, PrivateKey, ServerConfig};
use tokio_rustls::TlsAcceptor;
use tower::MakeService;
use tower_http::trace::TraceLayer;
use tracing::instrument;
use tracing::{info, trace};

use crate::storage::StorageError::{DataServerError, IoError};
use crate::storage::{configure_cors, UrlFormatter};

use super::{Result, StorageError};

/// The maximum amount of time a CORS request can be cached for.
pub const CORS_MAX_AGE: u64 = 86400;

/// A certificate and key pair used for tls.
/// This is the path to the PEM formatted X.509 certificate and private key.
#[derive(Debug, Clone)]
pub struct CertificateKeyPair {
  cert: PathBuf,
  key: PathBuf,
}

/// Ticket server url formatter.
#[derive(Debug, Clone)]
pub struct HttpTicketFormatter {
  addr: SocketAddr,
  cert_key_pair: Option<CertificateKeyPair>,
  scheme: Scheme,
  cors: CorsConfig,
}

impl HttpTicketFormatter {
  const SERVE_ASSETS_AT: &'static str = "/data";

  pub fn new(addr: SocketAddr, cors: CorsConfig) -> Self {
    Self {
      addr,
      cert_key_pair: None,
      scheme: Scheme::HTTP,
      cors,
    }
  }

  pub fn new_with_tls<P: AsRef<Path>>(addr: SocketAddr, cors: CorsConfig, cert: P, key: P) -> Self {
    Self {
      addr,
      cert_key_pair: Some(CertificateKeyPair {
        cert: PathBuf::from(cert.as_ref()),
        key: PathBuf::from(key.as_ref()),
      }),
      scheme: Scheme::HTTPS,
      cors,
    }
  }

  /// Get the scheme this formatter is using - either HTTP or HTTPS.
  pub fn get_scheme(&self) -> &Scheme {
    &self.scheme
  }

  /// Eagerly bind the address by returning a `DataServer`. This function also updates the
  /// address to the actual bound address, and replaces the cert_key_pair with None.
  pub async fn bind_data_server(&mut self) -> Result<DataServer> {
    let server = DataServer::bind_addr(
      self.addr,
      Self::SERVE_ASSETS_AT,
      self.cert_key_pair.take(),
      self.cors.clone(),
    )
    .await?;
    self.addr = server.local_addr();
    Ok(server)
  }

  /// Get the [SocketAddr] of this formatter.
  pub fn get_addr(&self) -> SocketAddr {
    self.addr
  }
}

impl TryFrom<DataServerConfig> for HttpTicketFormatter {
  type Error = StorageError;

  /// Returns a ticket server with tls if both cert and key are not None, without tls if cert and key
  /// are both None, and otherwise an error.
  fn try_from(config: DataServerConfig) -> Result<Self> {
    match (config.cert(), config.key()) {
      (Some(cert), Some(key)) => Ok(Self::new_with_tls(
        config.addr(),
        config.cors().clone(),
        cert,
        key,
      )),
      (Some(_), None) | (None, Some(_)) => Err(DataServerError(
        "both the cert and key must be provided for the ticket server".to_string(),
      )),
      (None, None) => Ok(Self::new(config.addr(), config.cors().clone())),
    }
  }
}

impl From<AddrParseError> for StorageError {
  fn from(err: AddrParseError) -> Self {
    StorageError::InvalidAddress(err)
  }
}

/// The local storage static http server.
#[derive(Debug)]
pub struct DataServer {
  listener: AddrIncoming,
  serve_assets_at: String,
  cert_key_pair: Option<CertificateKeyPair>,
  cors: CorsConfig,
}

impl DataServer {
  /// Eagerly bind the the address for use with the server, returning any errors.
  #[instrument(skip(serve_assets_at, cert_key_pair))]
  pub async fn bind_addr(
    addr: SocketAddr,
    serve_assets_at: impl Into<String>,
    cert_key_pair: Option<CertificateKeyPair>,
    cors: CorsConfig,
  ) -> Result<DataServer> {
    let listener = TcpListener::bind(addr)
      .await
      .map_err(|err| IoError("binding data server addr".to_string(), err))?;
    let listener = AddrIncoming::from_listener(listener)?;

    info!(address = ?listener.local_addr(), "data server address bound to");
    Ok(Self {
      listener,
      serve_assets_at: serve_assets_at.into(),
      cert_key_pair,
      cors,
    })
  }

  /// Run the actual server, using the provided path, key and certificate.
  #[instrument(level = "trace", skip_all)]
  pub async fn serve<P: AsRef<Path>>(mut self, path: P) -> Result<()> {
    let mut app = Router::new()
      .merge(SpaRouter::new(&self.serve_assets_at, path))
      .layer(configure_cors(self.cors)?)
      .layer(TraceLayer::new_for_http())
      .into_make_service_with_connect_info::<SocketAddr>();

    match self.cert_key_pair {
      None => axum::Server::builder(self.listener)
        .serve(app)
        .await
        .map_err(|err| DataServerError(err.to_string())),
      Some(CertificateKeyPair { cert, key }) => {
        let rustls_config = Self::rustls_server_config(key, cert)?;
        let acceptor = TlsAcceptor::from(rustls_config);

        loop {
          let stream = poll_fn(|cx| Pin::new(&mut self.listener).poll_accept(cx))
            .await
            .ok_or_else(|| DataServerError("poll accept failed".to_string()))?
            .map_err(|err| DataServerError(err.to_string()))?;
          let acceptor = acceptor.clone();

          let app = app
            .make_service(&stream)
            .await
            .map_err(|err| DataServerError(err.to_string()))?;

          trace!(stream = ?stream, "accepting stream");
          tokio::spawn(async move {
            if let Ok(stream) = acceptor.accept(stream).await {
              let _ = Http::new().serve_connection(stream, app).await;
            }
          });
        }
      }
    }
  }

  /// Get the local address the server has bound to.
  pub fn local_addr(&self) -> SocketAddr {
    self.listener.local_addr()
  }

  fn rustls_server_config<P: AsRef<Path>>(key: P, cert: P) -> Result<Arc<ServerConfig>> {
    let mut key_reader = BufReader::new(
      File::open(key).map_err(|err| IoError("failed to open key file".to_string(), err))?,
    );
    let mut cert_reader = BufReader::new(
      File::open(cert).map_err(|err| IoError("failed to open cert file".to_string(), err))?,
    );

    let key = PrivateKey(
      pkcs8_private_keys(&mut key_reader)
        .map_err(|err| IoError("failed to read private keys".to_string(), err))?
        .remove(0),
    );
    let certs = certs(&mut cert_reader)
      .map_err(|err| IoError("failed to read certificate".to_string(), err))?
      .into_iter()
      .map(Certificate)
      .collect();

    let mut config = ServerConfig::builder()
      .with_safe_defaults()
      .with_no_client_auth()
      .with_single_cert(certs, key)
      .map_err(|err| DataServerError(err.to_string()))?;

    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(Arc::new(config))
  }
}

impl From<hyper::Error> for StorageError {
  fn from(error: hyper::Error) -> Self {
    DataServerError(error.to_string())
  }
}

impl UrlFormatter for HttpTicketFormatter {
  fn format_url<K: AsRef<str>>(&self, key: K) -> Result<String> {
    http::uri::Builder::new()
      .scheme(self.get_scheme().clone())
      .authority(self.addr.to_string())
      .path_and_query(format!("{}/{}", Self::SERVE_ASSETS_AT, key.as_ref()))
      .build()
      .map_err(|err| StorageError::InvalidUri(err.to_string()))
      .map(|value| value.to_string())
  }
}

#[cfg(test)]
mod tests {
  use std::str::FromStr;

  use async_trait::async_trait;
  use http::header::HeaderName;
  use http::{HeaderMap, HeaderValue, Method};
  use reqwest::{Client, ClientBuilder, RequestBuilder};

  use htsget_test_utils::cors_tests::{
    test_cors_preflight_request_uri, test_cors_simple_request_uri,
  };
  use htsget_test_utils::http_tests::{
    default_cors_config, default_test_config, Header, Response as TestResponse, TestRequest,
    TestServer,
  };
  use htsget_test_utils::util::generate_test_certificates;

  use crate::storage::local::tests::create_local_test_files;
  use crate::Config;

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
    fn insert_header(mut self, header: Header<impl Into<String>>) -> Self {
      self.headers.insert(
        HeaderName::from_str(&header.name.into()).unwrap(),
        HeaderValue::from_str(&header.value.into()).unwrap(),
      );
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

    fn method(mut self, method: impl Into<String>) -> Self {
      self.method = method.into().parse().unwrap();
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
    fn get_config(&self) -> &Config {
      &self.config
    }

    fn get_request(&self) -> DataTestRequest {
      DataTestRequest::default()
    }

    async fn test_server(&self, request: DataTestRequest) -> TestResponse {
      let response = request.build().send().await.unwrap();
      let status: u16 = response.status().into();
      let headers = response.headers().clone();
      let bytes = response.bytes().await.unwrap().to_vec();

      TestResponse::new(status, headers, bytes, "".to_string())
    }
  }

  #[tokio::test]
  async fn test_http_server() {
    let (_, base_path) = create_local_test_files().await;

    test_server("http", None, base_path.path().to_path_buf()).await;
  }

  #[tokio::test]
  async fn test_tls_server() {
    let (_, base_path) = create_local_test_files().await;
    let (key_path, cert_path) = generate_test_certificates(base_path.path(), "key.pem", "cert.pem");

    test_server(
      "https",
      Some(CertificateKeyPair {
        cert: cert_path,
        key: key_path,
      }),
      base_path.path().to_path_buf(),
    )
    .await;
  }

  #[test]
  fn http_formatter_authority() {
    let formatter =
      HttpTicketFormatter::new("127.0.0.1:8080".parse().unwrap(), CorsConfig::default());
    test_formatter_authority(formatter, "http");
  }

  #[test]
  fn https_formatter_authority() {
    let formatter = HttpTicketFormatter::new_with_tls(
      "127.0.0.1:8080".parse().unwrap(),
      CorsConfig::default(),
      "",
      "",
    );
    test_formatter_authority(formatter, "https");
  }

  #[test]
  fn http_scheme() {
    let formatter =
      HttpTicketFormatter::new("127.0.0.1:8080".parse().unwrap(), CorsConfig::default());
    assert_eq!(formatter.get_scheme(), &Scheme::HTTP);
  }

  #[test]
  fn https_scheme() {
    let formatter = HttpTicketFormatter::new_with_tls(
      "127.0.0.1:8080".parse().unwrap(),
      CorsConfig::default(),
      "",
      "",
    );
    assert_eq!(formatter.get_scheme(), &Scheme::HTTPS);
  }

  #[tokio::test]
  async fn get_addr_local_addr() {
    let mut formatter =
      HttpTicketFormatter::new("127.0.0.1:0".parse().unwrap(), CorsConfig::default());
    let server = formatter.bind_data_server().await.unwrap();
    assert_eq!(formatter.get_addr(), server.local_addr());
  }

  #[tokio::test]
  async fn cors_simple_response() {
    let (_, base_path) = create_local_test_files().await;

    let port = start_server(None, base_path.path().to_path_buf()).await;

    test_cors_simple_request_uri(
      &DataTestServer::default(),
      &format!("http://localhost:{port}/data/key1"),
    )
    .await;
  }

  #[tokio::test]
  async fn cors_options_response() {
    let (_, base_path) = create_local_test_files().await;

    let port = start_server(None, base_path.path().to_path_buf()).await;

    test_cors_preflight_request_uri(
      &DataTestServer::default(),
      &format!("http://localhost:{port}/data/key1"),
    )
    .await;
  }

  async fn start_server<P>(cert_key_pair: Option<CertificateKeyPair>, path: P) -> u16
  where
    P: AsRef<Path> + Send + 'static,
  {
    let addr = SocketAddr::from_str(&format!("{}:{}", "127.0.0.1", "0")).unwrap();
    let server = DataServer::bind_addr(addr, "/data", cert_key_pair, default_cors_config())
      .await
      .unwrap();
    let port = server.local_addr().port();
    tokio::spawn(async move { server.serve(path).await.unwrap() });

    port
  }

  async fn test_server<P>(scheme: &str, cert_key_pair: Option<CertificateKeyPair>, path: P)
  where
    P: AsRef<Path> + Send + 'static,
  {
    let port = start_server(cert_key_pair, path).await;

    let test_server = DataTestServer::default();
    let request = test_server
      .get_request()
      .method(Method::GET.to_string())
      .uri(format!("{scheme}://localhost:{port}/data/key1"));
    let response = test_server.test_server(request).await;

    assert!(response.is_success());
    assert_eq!(response.body, b"value1");
  }

  fn test_formatter_authority(formatter: HttpTicketFormatter, scheme: &str) {
    assert_eq!(
      formatter.format_url("path").unwrap(),
      format!(
        "{}://127.0.0.1:8080{}/path",
        scheme,
        HttpTicketFormatter::SERVE_ASSETS_AT
      )
    )
  }
}
