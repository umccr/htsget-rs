use std::path::Path;
use axum::Router;
use axum_extra::routing::SpaRouter;
use tokio::task::JoinHandle;
use crate::storage::UrlFormatter;
use super::{GetOptions, Result, StorageError, UrlOptions};

/// The local storage static http server.
#[derive(Debug)]
pub struct LocalStorageServer {
  ip: String,
  port: String
}

impl LocalStorageServer {
  const SERVE_ASSETS_AT: &'static str = "/data";

  pub fn new(ip: impl Into<String>, port: impl Into<String>) -> Self {
    Self { ip: ip.into(), port: port.into() }
  }

  pub fn start_server<P: AsRef<Path>>(path: P, ip: String, port: String) -> JoinHandle<Result<()>> {
    let app = Router::new().merge(SpaRouter::new(Self::SERVE_ASSETS_AT, path));
    let ip_copy = ip.clone();
    let port_copy = port.clone();
    tokio::spawn(
        async move {
          axum::Server::bind(&format!("{}:{}", ip_copy, port_copy).parse().unwrap())
            .serve(app.into_make_service())
            .await.map_err(|err| StorageError::ResponseServerError(err.to_string()))
        })
  }
}

impl UrlFormatter for LocalStorageServer {
  fn format_url(&self, path: String) -> String {
    let builder = axum::http::uri::Builder::new();
    builder.scheme(self.format_scheme().as_str()).authority(self.format_authority()).path_and_query(path).build().expect("Expected valid uri.").to_string()
  }

  fn format_scheme(&self) -> String {
    axum::http::uri::Scheme::HTTPS.to_string()
  }

  fn format_authority(&self) -> String {
    format!("{}:{}", self.ip, self.port)
  }
}