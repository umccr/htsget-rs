use std::sync::Arc;

use actix_cors::Cors;
use actix_web::dev::Server;
use actix_web::{web, App, HttpServer};
use tracing::info;
use tracing::instrument;
use tracing_actix_web::TracingLogger;

use htsget_config::config::cors::CorsConfig;
pub use htsget_config::config::{Config, DataServerConfig, ServiceInfo, TicketServerConfig, USAGE};
pub use htsget_config::storage::Storage;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::htsget::HtsGet;
use htsget_search::storage::local::LocalStorage;

use crate::handlers::{get, post, reads_service_info, variants_service_info};

pub mod handlers;

pub type HtsGetStorage<T> = HtsGetFromStorage<LocalStorage<T>>;

/// Represents the actix app state.
pub struct AppState<H: HtsGet> {
  pub htsget: Arc<H>,
  pub config_service_info: ServiceInfo,
}

/// Configure the query server.
pub fn configure_server<H: HtsGet + Send + Sync + 'static>(
  service_config: &mut web::ServiceConfig,
  htsget: H,
  config_service_info: ServiceInfo,
) {
  service_config
    .app_data(web::Data::new(AppState {
      htsget: Arc::new(htsget),
      config_service_info,
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
    .service(
      web::scope("/portal")
        .route("/", web::get().to(get::portal::<H>)),
    );
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
  cors_layer = cors.allow_headers().apply_list(
    |cors_layer, headers| cors_layer.allowed_headers(headers.clone()),
    cors_layer,
  );

  cors_layer = cors
    .allow_methods()
    .apply_any(|cors_layer| cors_layer.allow_any_method(), cors_layer);
  cors_layer = cors.allow_methods().apply_list(
    |cors_layer, methods| cors_layer.allowed_methods(methods.clone()),
    cors_layer,
  );

  cors_layer = cors
    .expose_headers()
    .apply_any(|cors_layer| cors_layer.expose_any_header(), cors_layer);
  cors_layer = cors.expose_headers().apply_list(
    |cors_layer, headers| cors_layer.expose_headers(headers.clone()),
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
) -> std::io::Result<Server> {
  let addr = config.addr();

  let config_copy = config.clone();
  let server = HttpServer::new(Box::new(move || {
    App::new()
      .configure(|service_config: &mut web::ServiceConfig| {
        configure_server(service_config, htsget.clone(), service_info.clone());
      })
      .wrap(configure_cors(config_copy.cors().clone()))
      .wrap(TracingLogger::default())
  }));

  let server = match config.into_tls() {
    None => {
      info!("using non-TLS ticket server");
      server.bind(addr)?
    }
    Some(tls) => {
      info!("using TLS ticket server");
      server.bind_rustls_021(addr, tls.into_inner())?
    }
  };

  info!(addresses = ?server.addrs(), "htsget query server addresses bound");
  Ok(server.run())
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use actix_web::body::{BoxBody, EitherBody};
  use actix_web::dev::ServiceResponse;
  use actix_web::{test, web, App};
  use async_trait::async_trait;
  use tempfile::TempDir;

  use htsget_config::types::JsonResponse;
  use htsget_search::storage::data_server::BindDataServer;
  use htsget_test::http_tests::{config_with_tls, default_test_config};
  use htsget_test::http_tests::{
    Header as TestHeader, Response as TestResponse, TestRequest, TestServer,
  };
  use htsget_test::server_tests::expected_url_path;
  use htsget_test::{cors_tests, server_tests};

  use crate::Config;

  use super::*;

  struct ActixTestServer {
    config: Config,
  }

  struct ActixTestRequest<T>(T);

  impl TestRequest for ActixTestRequest<test::TestRequest> {
    fn insert_header(self, header: TestHeader<impl Into<String>>) -> Self {
      Self(self.0.insert_header(header.into_tuple()))
    }

    fn set_payload(self, payload: impl Into<String>) -> Self {
      Self(self.0.set_payload(payload.into()))
    }

    fn uri(self, uri: impl Into<String>) -> Self {
      Self(self.0.uri(&uri.into()))
    }

    fn method(self, method: impl Into<String>) -> Self {
      Self(
        self
          .0
          .method(method.into().parse().expect("expected valid method")),
      )
    }
  }

  impl Default for ActixTestServer {
    fn default() -> Self {
      Self {
        config: default_test_config(),
      }
    }
  }

  #[async_trait(?Send)]
  impl TestServer<ActixTestRequest<test::TestRequest>> for ActixTestServer {
    async fn get_expected_path(&self) -> String {
      let mut bind_data_server =
        BindDataServer::try_from(self.get_config().data_server().clone()).unwrap();
      let server = bind_data_server.bind_data_server().await.unwrap();
      let addr = server.local_addr();

      let path = self.get_config().data_server().local_path().to_path_buf();
      tokio::spawn(async move { server.serve(path).await.unwrap() });

      expected_url_path(self.get_config(), addr)
    }

    fn get_config(&self) -> &Config {
      &self.config
    }

    fn get_request(&self) -> ActixTestRequest<test::TestRequest> {
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
        headers
          .drain()
          .map(|(name, value)| (name.unwrap(), value))
          .collect(),
        bytes,
        expected_path,
      )
    }
  }

  impl ActixTestServer {
    fn new_with_tls<P: AsRef<Path>>(path: P) -> Self {
      Self {
        config: config_with_tls(path),
      }
    }

    async fn get_response(
      &self,
      request: test::TestRequest,
    ) -> ServiceResponse<EitherBody<BoxBody>> {
      let app = test::init_service(
        App::new()
          .configure(|service_config: &mut web::ServiceConfig| {
            configure_server(
              service_config,
              self.config.clone().owned_resolvers(),
              self.config.service_info().clone(),
            );
          })
          .wrap(configure_cors(self.config.ticket_server().cors().clone())),
      )
      .await;

      request.send_request(&app).await
    }
  }

  #[actix_web::test]
  async fn get_http_tickets() {
    server_tests::test_get::<JsonResponse, _>(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn post_http_tickets() {
    server_tests::test_post::<JsonResponse, _>(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn parameterized_get_http_tickets() {
    server_tests::test_parameterized_get::<JsonResponse, _>(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn parameterized_post_http_tickets() {
    server_tests::test_parameterized_post::<JsonResponse, _>(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn parameterized_post_class_header_http_tickets() {
    server_tests::test_parameterized_post_class_header::<JsonResponse, _>(
      &ActixTestServer::default(),
    )
    .await;
  }

  #[actix_web::test]
  async fn service_info() {
    server_tests::test_service_info(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn get_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_get::<JsonResponse, _>(&ActixTestServer::new_with_tls(base_path.path()))
      .await;
  }

  #[actix_web::test]
  async fn post_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_post::<JsonResponse, _>(&ActixTestServer::new_with_tls(base_path.path()))
      .await;
  }

  #[actix_web::test]
  async fn parameterized_get_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_parameterized_get::<JsonResponse, _>(&ActixTestServer::new_with_tls(
      base_path.path(),
    ))
    .await;
  }

  #[actix_web::test]
  async fn parameterized_post_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_parameterized_post::<JsonResponse, _>(&ActixTestServer::new_with_tls(
      base_path.path(),
    ))
    .await;
  }

  #[actix_web::test]
  async fn parameterized_post_class_header_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_parameterized_post_class_header::<JsonResponse, _>(
      &ActixTestServer::new_with_tls(base_path.path()),
    )
    .await;
  }

  #[actix_web::test]
  async fn cors_simple_request() {
    cors_tests::test_cors_simple_request(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn cors_preflight_request() {
    cors_tests::test_cors_preflight_request(&ActixTestServer::default()).await;
  }
}
