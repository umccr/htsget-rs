//! The Axum ticket server.
//!

use crate::error::Result;
use crate::handlers::{get, post, reads_service_info, variants_service_info};
use crate::middleware::auth::AuthLayer;
use crate::server::{configure_cors, AppState, BindServer, Server};
use axum::routing::get;
use axum::Router;
use htsget_config::config::advanced::auth::AuthConfig;
use htsget_config::config::advanced::cors::CorsConfig;
use htsget_config::config::service_info::ServiceInfo;
use htsget_config::config::ticket_server::TicketServerConfig;
use htsget_config::config::Config;
use htsget_http::middleware::auth::AuthBuilder;
use htsget_search::HtsGet;
use http::{StatusCode, Uri};
use std::net::SocketAddr;
use tokio::task::JoinHandle;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;

impl From<TicketServerConfig> for BindServer {
  /// Returns a ticket server with TLS enabled if the tls config is not None or without TLS enabled
  /// if it is None.
  fn from(config: TicketServerConfig) -> Self {
    let addr = config.addr();
    let cors = config.cors().clone();
    let auth = config.auth().cloned();

    match config.into_tls() {
      None => Self::new(addr, cors, auth),
      Some(tls) => Self::new_with_tls(addr, cors, auth, tls),
    }
  }
}

/// An data block server.
#[derive(Debug)]
pub struct TicketServer<H> {
  server: Server,
  htsget: H,
  service_info: ServiceInfo,
  cors: CorsConfig,
  auth: Option<AuthConfig>,
}

impl<H> TicketServer<H>
where
  H: HtsGet + Clone + Send + Sync + 'static,
{
  /// Create a new ticket server.
  pub fn new(
    server: Server,
    htsget: H,
    service_info: ServiceInfo,
    cors: CorsConfig,
    auth: Option<AuthConfig>,
  ) -> Self {
    Self {
      server,
      htsget,
      service_info,
      cors,
      auth,
    }
  }

  /// Run the data server, using the key and certificate.
  pub async fn serve(self) -> Result<()> {
    self
      .server
      .serve(Self::router(
        self.htsget,
        self.service_info,
        self.cors,
        self.auth,
      )?)
      .await
  }

  /// Create the router for the ticket server.
  pub fn router(
    htsget: H,
    service_info: ServiceInfo,
    cors: CorsConfig,
    auth: Option<AuthConfig>,
  ) -> Result<Router> {
    let router = Router::default()
      .route(
        "/reads/service-info",
        get(reads_service_info::<H>).post(reads_service_info::<H>),
      )
      .route("/reads/{*id}", get(get::reads).post(post::reads))
      .route(
        "/variants/service-info",
        get(variants_service_info::<H>).post(variants_service_info::<H>),
      )
      .route("/variants/{*id}", get(get::variants).post(post::variants))
      .fallback(Self::fallback)
      .layer(
        ServiceBuilder::new()
          .layer(TraceLayer::new_for_http())
          .layer(configure_cors(cors)),
      )
      .with_state(AppState::new(htsget, service_info));

    if let Some(auth) = auth {
      Ok(router.layer(AuthLayer::from(
        AuthBuilder::default().with_config(auth).build()?,
      )))
    } else {
      Ok(router)
    }
  }

  /// Get the local address the server has bound to.
  pub fn local_addr(&self) -> Result<SocketAddr> {
    self.server.local_addr()
  }

  /// A handler for when a route is not found.
  async fn fallback(uri: Uri) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, format!("No route for {uri}"))
  }
}

/// Spawn a task to run the ticket server.
pub async fn join_handle(config: Config) -> Result<JoinHandle<Result<()>>> {
  let service_info = config.service_info().clone();
  let ticket_server = BindServer::from(config.ticket_server().clone())
    .bind_ticket_server(config.into_locations(), service_info)
    .await?;

  info!(address = ?ticket_server.local_addr()?, "ticket server address bound to");

  Ok(tokio::spawn(async move { ticket_server.serve().await }))
}

#[cfg(test)]
mod tests {
  use std::convert::Infallible;
  use std::path::Path;
  use std::result;

  use super::*;
  use async_trait::async_trait;
  use axum::body::{to_bytes, Body};
  use axum::response::Response;
  use htsget_config::config::Config;
  use htsget_config::types::JsonResponse;
  use htsget_test::http::server::expected_url_path;
  use htsget_test::http::{
    config_with_tls, cors, default_test_config, server, Header, Response as TestResponse,
    TestRequest, TestServer,
  };
  use http::header::HeaderName;
  use http::{Method, Request};
  use rustls::crypto::aws_lc_rs;
  use tempfile::TempDir;
  use tower::ServiceExt;

  struct AxumTestServer {
    config: Config,
  }

  struct AxumTestRequest<T>(T);

  impl TestRequest for AxumTestRequest<Request<Body>> {
    fn insert_header(
      mut self,
      header: Header<impl Into<HeaderName>, impl Into<http::HeaderValue>>,
    ) -> Self {
      self
        .0
        .headers_mut()
        .insert(header.name.into(), header.value.into());
      self
    }

    fn set_payload(mut self, payload: impl Into<String>) -> Self {
      *self.0.body_mut() = Body::new(payload.into());
      self
    }

    fn uri(mut self, uri: impl Into<String>) -> Self {
      let uri = uri.into();
      *self.0.uri_mut() = uri.parse().expect("expected valid uri");
      self
    }

    fn method(mut self, method: impl Into<Method>) -> Self {
      *self.0.method_mut() = method.into();
      self
    }
  }

  impl Default for AxumTestServer {
    fn default() -> Self {
      Self {
        config: default_test_config(),
      }
    }
  }

  #[async_trait(?Send)]
  impl TestServer<AxumTestRequest<Request<Body>>> for AxumTestServer {
    async fn get_expected_path(&self) -> String {
      let data_server = self
        .get_config()
        .data_server()
        .as_data_server_config()
        .unwrap();

      let path = data_server.local_path().to_path_buf();
      let mut bind_data_server = BindServer::from(data_server.clone());
      let server = bind_data_server.bind_data_server().await.unwrap();
      let addr = server.local_addr();

      tokio::spawn(async move { server.serve(path).await.unwrap() });

      expected_url_path(self.get_config(), addr.unwrap())
    }

    fn get_config(&self) -> &Config {
      &self.config
    }

    fn request(&self) -> AxumTestRequest<Request<Body>> {
      AxumTestRequest(Request::default())
    }

    async fn test_server(
      &self,
      request: AxumTestRequest<Request<Body>>,
      expected_path: String,
    ) -> TestResponse {
      let response = self.get_response(request.0).await.unwrap();

      let status: u16 = response.status().into();
      let headers = response.headers().clone();
      let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();

      TestResponse::new(status, headers, bytes, expected_path)
    }
  }

  impl AxumTestServer {
    fn new_with_tls<P: AsRef<Path>>(path: P) -> Self {
      let _ = aws_lc_rs::default_provider().install_default();

      Self {
        config: config_with_tls(path),
      }
    }

    async fn get_response(&self, request: Request<Body>) -> result::Result<Response, Infallible> {
      let app = TicketServer::router(
        self.config.clone().into_locations(),
        self.config.service_info().clone(),
        self.config.ticket_server().cors().clone(),
        None,
      )
      .unwrap();

      app.oneshot(request).await
    }
  }

  #[tokio::test]
  async fn get_http_tickets() {
    server::test_get::<JsonResponse, _>(&AxumTestServer::default()).await;
  }

  #[tokio::test]
  async fn post_http_tickets() {
    server::test_post::<JsonResponse, _>(&AxumTestServer::default()).await;
  }

  #[tokio::test]
  async fn parameterized_get_http_tickets() {
    server::test_parameterized_get::<JsonResponse, _>(&AxumTestServer::default()).await;
  }

  #[tokio::test]
  async fn parameterized_post_http_tickets() {
    server::test_parameterized_post::<JsonResponse, _>(&AxumTestServer::default()).await;
  }

  #[tokio::test]
  async fn parameterized_post_class_header_http_tickets() {
    server::test_parameterized_post_class_header::<JsonResponse, _>(&AxumTestServer::default())
      .await;
  }

  #[tokio::test]
  async fn service_info() {
    server::test_service_info(&AxumTestServer::default()).await;
  }

  #[tokio::test]
  async fn get_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server::test_get::<JsonResponse, _>(&AxumTestServer::new_with_tls(base_path.path())).await;
  }

  #[tokio::test]
  async fn post_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server::test_post::<JsonResponse, _>(&AxumTestServer::new_with_tls(base_path.path())).await;
  }

  #[tokio::test]
  async fn parameterized_get_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server::test_parameterized_get::<JsonResponse, _>(&AxumTestServer::new_with_tls(
      base_path.path(),
    ))
    .await;
  }

  #[tokio::test]
  async fn parameterized_post_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server::test_parameterized_post::<JsonResponse, _>(&AxumTestServer::new_with_tls(
      base_path.path(),
    ))
    .await;
  }

  #[tokio::test]
  async fn parameterized_post_class_header_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server::test_parameterized_post_class_header::<JsonResponse, _>(&AxumTestServer::new_with_tls(
      base_path.path(),
    ))
    .await;
  }

  #[tokio::test]
  async fn cors_simple_request() {
    cors::test_cors_simple_request(&AxumTestServer::default()).await;
  }

  #[tokio::test]
  async fn cors_preflight_request() {
    cors::test_cors_preflight_request(&AxumTestServer::default()).await;
  }

  #[tokio::test]
  async fn test_errors() {
    server::test_errors(&AxumTestServer::default()).await;
  }
}
