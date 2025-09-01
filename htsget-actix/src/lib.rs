extern crate core;

use crate::handlers::{
  HttpVersionCompat, get, handle_response, post, reads_service_info, variants_service_info,
};
use actix_cors::Cors;
use actix_web::dev::Server;
use actix_web::{App, HttpRequest, HttpServer, Responder, web};
use htsget_config::config::advanced::cors::CorsConfig;
use htsget_config::config::service_info::ServiceInfo;
use htsget_config::config::ticket_server::TicketServerConfig;
pub use htsget_config::config::{Config, USAGE};
use htsget_http::error::HtsGetError;
use htsget_http::middleware::auth::{Auth, AuthBuilder};
use htsget_search::HtsGet;
use std::io;
use tracing::info;
use tracing::instrument;
use tracing_actix_web::TracingLogger;

pub mod handlers;

/// Represents the actix app state.
pub struct AppState<H: HtsGet> {
  pub htsget: H,
  pub config_service_info: ServiceInfo,
  pub auth: Option<Auth>,
}

/// Configure the query server.
pub fn configure_server<H: HtsGet + Clone + Send + Sync + 'static>(
  service_config: &mut web::ServiceConfig,
  htsget: H,
  config_service_info: ServiceInfo,
  auth: Option<Auth>,
) {
  service_config
    .app_data(web::Data::new(AppState {
      htsget,
      config_service_info,
      auth,
    }))
    .service(
      web::scope("/reads")
        .route("/service-info", web::get().to(reads_service_info::<H>))
        .route("/service-info", web::post().to(reads_service_info::<H>))
        .route("/{id:.+}", web::get().to(get::reads::<H>))
        .route("/{id:.+}", web::post().to(post::reads::<H>)),
    )
    .service(
      web::scope("/variants")
        .route("/service-info", web::get().to(variants_service_info::<H>))
        .route("/service-info", web::post().to(variants_service_info::<H>))
        .route("/{id:.+}", web::get().to(get::variants::<H>))
        .route("/{id:.+}", web::post().to(post::variants::<H>)),
    )
    .default_service(web::to(fallback));
}

/// A handler for when a route is not found.
async fn fallback(http_request: HttpRequest) -> impl Responder {
  handle_response(Err(HtsGetError::NotFound(format!(
    "No route for {}",
    http_request.uri()
  ))))
}

/// Configure cors, settings allowed methods, max age, allowed origins, and if credentials
/// are supported.
pub fn configure_cors(cors: CorsConfig) -> Cors {
  let mut cors_layer = Cors::default();
  cors_layer = cors.allow_origins().apply_any(
    |cors_layer| cors_layer.allow_any_origin().send_wildcard(),
    cors_layer,
  );
  cors_layer = cors
    .allow_origins()
    .apply_mirror(|cors_layer| cors_layer.allow_any_origin(), cors_layer);
  cors_layer = cors.allow_origins().apply_list(
    |mut cors_layer, origins| {
      for origin in origins {
        cors_layer = cors_layer.allowed_origin(&origin.to_string());
      }
      cors_layer
    },
    cors_layer,
  );

  cors_layer = cors
    .allow_headers()
    .apply_any(|cors_layer| cors_layer.allow_any_header(), cors_layer);
  cors_layer = cors
    .allow_headers()
    .apply_mirror(|cors_layer| cors_layer.allow_any_header(), cors_layer);
  cors_layer = cors.allow_headers().apply_list(
    |cors_layer, headers| {
      cors_layer.allowed_headers(HttpVersionCompat::header_names_1_to_0_2(headers.clone()))
    },
    cors_layer,
  );

  cors_layer = cors
    .allow_methods()
    .apply_any(|cors_layer| cors_layer.allow_any_method(), cors_layer);
  cors_layer = cors
    .allow_methods()
    .apply_mirror(|cors_layer| cors_layer.allow_any_method(), cors_layer);
  cors_layer = cors.allow_methods().apply_list(
    |cors_layer, methods| {
      cors_layer.allowed_methods(HttpVersionCompat::methods_0_2_to_1(methods.clone()))
    },
    cors_layer,
  );

  cors_layer = cors
    .expose_headers()
    .apply_any(|cors_layer| cors_layer.expose_any_header(), cors_layer);
  cors_layer = cors.expose_headers().apply_list(
    |cors_layer, headers| {
      cors_layer.expose_headers(HttpVersionCompat::header_names_1_to_0_2(headers.clone()))
    },
    cors_layer,
  );

  if cors.allow_credentials() {
    cors_layer = cors_layer.supports_credentials();
  }

  cors_layer.max_age(cors.max_age())
}

/// Run the server using a http-actix `HttpServer`.
#[instrument(skip_all)]
pub fn run_server<H: HtsGet + Clone + Send + Sync + 'static>(
  htsget: H,
  config: TicketServerConfig,
  service_info: ServiceInfo,
) -> io::Result<Server> {
  let app =
    |htsget: H, config: TicketServerConfig, service_info: ServiceInfo, auth: Option<Auth>| {
      App::new()
        .configure(|service_config: &mut web::ServiceConfig| {
          configure_server(service_config, htsget, service_info, auth);
        })
        .wrap(configure_cors(config.cors().clone()))
        .wrap(TracingLogger::default())
    };

  let auth = config
    .auth()
    .cloned()
    .map(|auth| AuthBuilder::default().with_config(auth).build())
    .transpose()
    .map_err(io::Error::other)?;
  let addr = config.addr();
  let config_copy = config.clone();
  let server = HttpServer::new(move || {
    app(
      htsget.clone(),
      config_copy.clone(),
      service_info.clone(),
      auth.clone(),
    )
  });

  let server = match config.into_tls() {
    None => {
      info!("using non-TLS ticket server");
      server.bind(addr)?
    }
    Some(tls) => {
      info!("using TLS ticket server");
      server.bind_rustls_0_23(addr, tls.into_inner())?
    }
  };

  info!(addresses = ?server.addrs(), "htsget query server addresses bound");
  Ok(server.run())
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use actix_web::body::BoxBody;
  use actix_web::dev::ServiceResponse;
  use actix_web::{App, test, web};
  use async_trait::async_trait;
  use htsget_test::http::auth::create_test_auth_config;
  use rustls::crypto::aws_lc_rs;
  use tempfile::TempDir;

  use crate::Config;
  use htsget_axum::server::BindServer;
  use htsget_config::storage::file::default_path;
  use htsget_config::types::JsonResponse;
  use htsget_http::middleware::auth::AuthBuilder;
  use htsget_test::http::auth::MockAuthServer;
  use htsget_test::http::server::expected_url_path;
  use htsget_test::http::{
    Header as TestHeader, Response as TestResponse, TestRequest, TestServer,
  };
  use htsget_test::http::{auth, config_with_tls, default_test_config};
  use htsget_test::http::{cors, server};
  use htsget_test::util::generate_key_pair;

  use super::*;

  struct ActixTestServer {
    config: Config,
    auth: Option<Auth>,
  }

  struct ActixTestRequest<T>(T);

  impl TestRequest for ActixTestRequest<test::TestRequest> {
    fn insert_header(
      self,
      header: TestHeader<impl Into<http_1::HeaderName>, impl Into<http_1::HeaderValue>>,
    ) -> Self {
      let (name, value) = header.into_tuple();
      Self(
        self
          .0
          .insert_header((name.to_string(), value.to_str().unwrap())),
      )
    }

    fn set_payload(self, payload: impl Into<String>) -> Self {
      Self(self.0.set_payload(payload.into()))
    }

    fn uri(self, uri: impl Into<String>) -> Self {
      Self(self.0.uri(&uri.into()))
    }

    fn method(self, method: impl Into<http_1::Method>) -> Self {
      Self(
        self.0.method(
          method
            .into()
            .to_string()
            .parse()
            .expect("expected valid method"),
        ),
      )
    }
  }

  impl Default for ActixTestServer {
    fn default() -> Self {
      Self {
        config: default_test_config(None),
        auth: None,
      }
    }
  }

  #[async_trait(?Send)]
  impl TestServer<ActixTestRequest<test::TestRequest>> for ActixTestServer {
    async fn get_expected_path(&self) -> String {
      let data_server = self
        .get_config()
        .data_server()
        .as_data_server_config()
        .unwrap();

      let path = data_server
        .local_path()
        .unwrap_or_else(|| default_path().as_ref())
        .to_path_buf();
      let mut bind_data_server = BindServer::from(data_server.clone());
      let server = bind_data_server.bind_data_server().await.unwrap();
      let addr = server.local_addr();

      tokio::spawn(async move { server.serve(path).await.unwrap() });

      expected_url_path(self.get_config(), addr.unwrap())
    }

    fn get_config(&self) -> &Config {
      &self.config
    }

    fn request(&self) -> ActixTestRequest<test::TestRequest> {
      ActixTestRequest(test::TestRequest::default())
    }

    async fn test_server(
      &self,
      request: ActixTestRequest<test::TestRequest>,
      expected_path: String,
    ) -> TestResponse {
      let response = self.get_response(request.0).await;

      let status: u16 = response.status().into();
      let mut headers = response.headers().clone();
      let bytes = test::read_body(response).await.to_vec();

      TestResponse::new(
        status,
        HttpVersionCompat::header_map_0_2_to_1(
          headers
            .drain()
            .map(|(name, value)| (name.unwrap(), value))
            .collect(),
        ),
        bytes,
        expected_path,
      )
    }
  }

  impl ActixTestServer {
    fn new_with_tls<P: AsRef<Path>>(path: P) -> Self {
      let _ = aws_lc_rs::default_provider().install_default();

      Self {
        config: config_with_tls(path),
        auth: None,
      }
    }

    async fn new_with_auth(public_key: Vec<u8>, suppressed: bool) -> Self {
      let mock_server = MockAuthServer::new().await;
      let auth_config = create_test_auth_config(&mock_server, public_key, suppressed);
      let auth = AuthBuilder::default()
        .with_config(auth_config.clone())
        .build()
        .unwrap();

      Self {
        config: default_test_config(Some(auth_config)),
        auth: Some(auth),
      }
    }

    async fn get_response(&self, request: test::TestRequest) -> ServiceResponse<BoxBody> {
      let app = App::new()
        .configure(|service_config: &mut web::ServiceConfig| {
          configure_server(
            service_config,
            self.config.clone().into_locations(),
            self.config.service_info().clone(),
            self.auth.clone(),
          );
        })
        .wrap(configure_cors(self.config.ticket_server().cors().clone()));

      let app = test::init_service(app).await;
      request.send_request(&app).await.map_into_boxed_body()
    }
  }

  #[actix_web::test]
  async fn get_http_tickets() {
    server::test_get::<JsonResponse, _>(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn post_http_tickets() {
    server::test_post::<JsonResponse, _>(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn parameterized_get_http_tickets() {
    server::test_parameterized_get::<JsonResponse, _>(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn parameterized_post_http_tickets() {
    server::test_parameterized_post::<JsonResponse, _>(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn parameterized_post_class_header_http_tickets() {
    server::test_parameterized_post_class_header::<JsonResponse, _>(&ActixTestServer::default())
      .await;
  }

  #[actix_web::test]
  async fn service_info() {
    server::test_service_info(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn get_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server::test_get::<JsonResponse, _>(&ActixTestServer::new_with_tls(base_path.path())).await;
  }

  #[actix_web::test]
  async fn post_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server::test_post::<JsonResponse, _>(&ActixTestServer::new_with_tls(base_path.path())).await;
  }

  #[actix_web::test]
  async fn parameterized_get_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server::test_parameterized_get::<JsonResponse, _>(&ActixTestServer::new_with_tls(
      base_path.path(),
    ))
    .await;
  }

  #[actix_web::test]
  async fn parameterized_post_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server::test_parameterized_post::<JsonResponse, _>(&ActixTestServer::new_with_tls(
      base_path.path(),
    ))
    .await;
  }

  #[actix_web::test]
  async fn parameterized_post_class_header_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server::test_parameterized_post_class_header::<JsonResponse, _>(
      &ActixTestServer::new_with_tls(base_path.path()),
    )
    .await;
  }

  #[actix_web::test]
  async fn cors_simple_request() {
    cors::test_cors_simple_request(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn cors_preflight_request() {
    cors::test_cors_preflight_request(&ActixTestServer::default(), "x-requested-with", "POST")
      .await;
  }

  #[actix_web::test]
  async fn test_auth_insufficient_permissions() {
    let (private_key, public_key) = generate_key_pair();

    let server = ActixTestServer::new_with_auth(public_key, false).await;
    auth::test_auth_insufficient_permissions::<JsonResponse, _>(&server, private_key).await;
  }

  #[actix_web::test]
  async fn test_auth_succeeds() {
    let (private_key, public_key) = generate_key_pair();

    auth::test_auth_succeeds::<JsonResponse, _>(
      &ActixTestServer::new_with_auth(public_key, false).await,
      private_key,
    )
    .await;
  }

  #[cfg(feature = "experimental")]
  #[actix_web::test]
  async fn test_auth_insufficient_permissions_suppressed() {
    let (private_key, public_key) = generate_key_pair();

    let server = ActixTestServer::new_with_auth(public_key, true).await;
    auth::test_auth_insufficient_permissions::<JsonResponse, _>(&server, private_key).await;
  }

  #[cfg(feature = "experimental")]
  #[actix_web::test]
  async fn test_auth_succeeds_suppressed() {
    let (private_key, public_key) = generate_key_pair();

    auth::test_auth_succeeds::<JsonResponse, _>(
      &ActixTestServer::new_with_auth(public_key, true).await,
      private_key,
    )
    .await;
  }
}
