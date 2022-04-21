use std::net::{AddrParseError, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use axum::{Error, Router};
use axum::routing::get;
use axum_extra::routing::SpaRouter;
use axum_server::tls_rustls::RustlsConfig;
use http::uri::Scheme;
use tokio::task::JoinHandle;
use crate::storage::{Storage, UrlFormatter};
use super::{GetOptions, Result, StorageError, UrlOptions};

/// The local storage static http server.
#[derive(Debug)]
pub struct LocalStorageServer {
  ip: String,
  port: String,
  cert_path: Option<PathBuf>,
  key_path: Option<PathBuf>,
  scheme: axum::http::uri::Scheme
}

impl LocalStorageServer {
  const SERVE_ASSETS_AT: &'static str = "/data";

  pub fn new<P: AsRef<Path>>(ip: impl Into<String>, port: impl Into<String>, cert_path: Option<PathBuf>, key_path: Option<PathBuf>) -> Self {
    let scheme = if let (Some(_), Some(_)) = (&cert_path, &key_path) {
      axum::http::uri::Scheme::HTTPS
    } else {
      axum::http::uri::Scheme::HTTP
    };

    Self { ip: ip.into(), port: port.into(), cert_path, key_path, scheme }
  }

  pub async fn start_server<P: AsRef<Path>>(&self, path: P) -> Result<JoinHandle<Result<()>>> {
    let app = Router::new().merge(SpaRouter::new(Self::SERVE_ASSETS_AT, path));

    let addr = format!("{}:{}", self.ip, self.port).parse::<SocketAddr>().map_err(|err| StorageError::ResponseServerError(err.to_string()))?;

    let cert_path = self.cert_path.clone();
    let key_path = self.key_path.clone();
    Ok(tokio::spawn(async move { Ok(
      if let (Some(cert_path), Some(key_path)) = (cert_path, key_path) {
        let config = RustlsConfig::from_pem_file(
          cert_path,
          key_path,
        ).await.map_err(|err| StorageError::ResponseServerError(err.to_string()))?;
        axum_server::bind_rustls(addr, config).serve(app.into_make_service()).await?
      } else {
        axum_server::bind(addr).serve(app.into_make_service()).await?
      }
    )}))
  }
}

impl From<hyper::Error> for StorageError {
  fn from(error: hyper::Error) -> Self {
    StorageError::ResponseServerError(error.to_string())
  }
}

impl From<AddrParseError> for StorageError {
  fn from(error: AddrParseError) -> Self {
    StorageError::InvalidInput(error.to_string())
  }
}

impl UrlFormatter for LocalStorageServer {
  fn format_url(&self, path: String) -> String {
    let builder = axum::http::uri::Builder::new();
    builder.scheme(self.format_scheme().as_str()).authority(self.format_authority()).path_and_query(path).build().expect("Expected valid uri.").to_string()
  }

  fn format_scheme(&self) -> String {
    self.scheme.to_string()
  }

  fn format_authority(&self) -> String {
    format!("{}:{}", self.ip, self.port)
  }
}

#[cfg(test)]
mod tests {
  use std::future::Future;
  use std::{matches, time};
  use std::net::{SocketAddr, TcpListener};
  use std::thread::sleep;
  use axum::routing::get;
  use http::Request;
  use hyper::Body;

  use tempfile::TempDir;
  use tokio::fs::{create_dir, File};
  use tokio::io::AsyncWriteExt;

  use crate::htsget::{Headers, Url};
  use crate::storage::{BytesRange, GetOptions, StorageError, UrlOptions};
  use crate::storage::local::tests::create_local_test_files;

  use super::*;

  #[tokio::test]
  async fn test_start_server() {
    let (_, base_path) = create_local_test_files().await;

    LocalStorageServer::new("127.0.0.1", "8080").start_server(base_path.path()).unwrap();

    let client = hyper::Client::new();
    let request = Request::builder()
      .uri(format!("http://{}:{}/data/key1", "127.0.0.1", "8080"))
      .body(Body::empty())
      .unwrap();
    let response = client
      .request(request)
      .await
      .unwrap();

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    assert_eq!(body.as_ref(), b"value1");
  }
}