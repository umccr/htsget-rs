use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use axum_extra::routing::SpaRouter;
use futures_util::future::poll_fn;
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, Http};
use rustls_pemfile::{certs, pkcs8_private_keys};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::rustls::{Certificate, PrivateKey, ServerConfig};
use tokio_rustls::TlsAcceptor;
use tower::MakeService;
use crate::storage::StorageError::ResponseServerError;

use crate::storage::UrlFormatter;

use super::{Result, StorageError};

/// The local storage static http server.
#[derive(Debug)]
pub struct AxumStorageServer {
  ip: String,
  port: String,
  listener: AddrIncoming
}

impl AxumStorageServer {
  const SERVE_ASSETS_AT: &'static str = "/data";

  /// Eagerly bind the the ip and port for use with the server, returning any errors.
  pub async fn bind_addr(ip: impl Into<String>, port: impl Into<String>) -> Result<Self> {
    let ip = ip.into();
    let port = port.into();
    let listener = TcpListener::bind(format!("{}:{}", ip, port)).await?;
    let listener = AddrIncoming::from_listener(listener).unwrap();
    Ok(Self {
      ip,
      port,
      listener
    })
  }

  /// Run the actual server, using the provided path, key and certificate.
  pub async fn serve<P: AsRef<Path>>(&mut self, path: P, key: P, cert: P) -> Result<()> {
    let mut app = Router::new()
      .merge(SpaRouter::new(Self::SERVE_ASSETS_AT, path))
      .into_make_service_with_connect_info::<SocketAddr>();

    let rustls_config = Self::rustls_server_config(key, cert)?;
    let acceptor = TlsAcceptor::from(rustls_config);

    loop {
      let stream = poll_fn(|cx| Pin::new(&mut self.listener).poll_accept(cx))
        .await
        .ok_or_else(|| ResponseServerError("Poll accept failed.".to_string()))?
        .map_err(|err| ResponseServerError(err.to_string()))?;
      let acceptor = acceptor.clone();

      let app = app.make_service(&stream).await.unwrap();

      tokio::spawn(async move {
        if let Ok(stream) = acceptor.accept(stream).await {
          let _ = Http::new().serve_connection(stream, app).await;
        }
      });
    }
  }

  fn rustls_server_config<P: AsRef<Path>>(key: P, cert: P) -> Result<Arc<ServerConfig>> {
    let mut key_reader = BufReader::new(File::open(key)?);
    let mut cert_reader = BufReader::new(File::open(cert)?);

    let key = PrivateKey(pkcs8_private_keys(&mut key_reader)?.remove(0));
    let certs = certs(&mut cert_reader)?
      .into_iter()
      .map(Certificate)
      .collect();

    let mut config = ServerConfig::builder()
      .with_safe_defaults()
      .with_no_client_auth()
      .with_single_cert(certs, key)
      .map_err(|err| StorageError::ResponseServerError(err.to_string()))?;

    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(Arc::new(config))
  }
}

impl From<hyper::Error> for StorageError {
  fn from(error: hyper::Error) -> Self {
    ResponseServerError(error.to_string())
  }
}

impl UrlFormatter for AxumStorageServer {
  fn format_url(&self, path: String) -> String {
    let builder = axum::http::uri::Builder::new();
    builder
      .scheme(self.format_scheme().as_str())
      .authority(self.format_authority())
      .path_and_query(path)
      .build()
      .expect("Expected valid uri.")
      .to_string()
  }

  fn format_scheme(&self) -> String {
    http::uri::Scheme::HTTPS.to_string()
  }

  fn format_authority(&self) -> String {
    format!("{}:{}", self.ip, self.port)
  }
}

#[cfg(test)]
mod tests {
  use http::Request;
  use hyper::Body;

  use crate::storage::local::tests::create_local_test_files;

  use super::*;

  #[tokio::test]
  async fn test_start_server() {
    let (_, base_path) = create_local_test_files().await;

    AxumStorageServer::new("127.0.0.1", "8080")
      .start_server(base_path.path())
      .unwrap();

    let client = hyper::Client::new();
    let request = Request::builder()
      .uri(format!("http://{}:{}/data/key1", "127.0.0.1", "8080"))
      .body(Body::empty())
      .unwrap();
    let response = client.request(request).await.unwrap();

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    assert_eq!(body.as_ref(), b"value1");
  }
}
