use std::sync::Arc;

use actix_cors::Cors;
use actix_web::dev::Server;
use actix_web::{web, App, HttpServer};
use tracing::info;
use tracing::instrument;
use tracing_actix_web::TracingLogger;

use htsget_config::config::{ServiceInfo, TicketServerConfig};
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::htsget::HtsGet;
use htsget_search::storage::local::LocalStorage;

use crate::handlers::{get, post, reads_service_info, variants_service_info};

pub mod handlers;

pub type HtsGetStorage<T> = HtsGetFromStorage<LocalStorage<T>>;

/// The maximum amount of time a CORS request can be cached for.
pub const CORS_MAX_AGE: usize = 86400;

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
    );
}

/// Configure cors, settings allowed methods, max age, allowed origins, and if credentials
/// are supported.
pub fn configure_cors(cors_allow_credentials: bool, cors_allow_origin: String) -> Cors {
  let cors = Cors::default()
    .allow_any_method()
    .allow_any_header()
    .allowed_origin(&cors_allow_origin)
    .max_age(CORS_MAX_AGE);

  if cors_allow_credentials {
    cors.supports_credentials()
  } else {
    cors
  }
}

/// Run the server using a http-actix `HttpServer`.
#[instrument(skip_all)]
pub fn run_server<H: HtsGet + Clone + Send + Sync + 'static>(
  htsget: H,
  config: TicketServerConfig,
) -> std::io::Result<Server> {
  let server = HttpServer::new(Box::new(move || {
    App::new()
      .configure(|service_config: &mut web::ServiceConfig| {
        configure_server(service_config, htsget.clone(), config.service_info.clone());
      })
      .wrap(configure_cors(
        config.ticket_server_cors_allow_credentials,
        config.ticket_server_cors_allow_origin.clone(),
      ))
      .wrap(TracingLogger::default())
  }))
  .bind(config.ticket_server_addr)?;

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

  use htsget_config::config::Config;
  use htsget_search::storage::data_server::HttpTicketFormatter;
  use htsget_test_utils::http_tests::{config_with_tls, default_test_config};
  use htsget_test_utils::http_tests::{
    Header as TestHeader, Response as TestResponse, TestRequest, TestServer,
  };
  use htsget_test_utils::server_tests::formatter_and_expected_path;
  use htsget_test_utils::{cors_tests, server_tests};

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
    fn get_config(&self) -> &Config {
      &self.config
    }

    fn get_request(&self) -> ActixTestRequest<test::TestRequest> {
      ActixTestRequest(test::TestRequest::default())
    }

    async fn test_server(&self, request: ActixTestRequest<test::TestRequest>) -> TestResponse {
      let (expected_path, formatter) = formatter_and_expected_path(self.get_config()).await;

      let response = self.get_response(request.0, formatter).await;
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
      formatter: HttpTicketFormatter,
    ) -> ServiceResponse<EitherBody<BoxBody>> {
      let app = test::init_service(
        App::new()
          .configure(|service_config: &mut web::ServiceConfig| {
            configure_server(
              service_config,
              HtsGetFromStorage::local_from(
                self.config.path.clone(),
                self.config.resolver.clone(),
                formatter,
              )
              .unwrap(),
              self.config.ticket_server_config.service_info.clone(),
            );
          })
          .wrap(configure_cors(
            self
              .config
              .data_server_config
              .data_server_cors_allow_credentials,
            self
              .config
              .data_server_config
              .data_server_cors_allow_origin
              .clone(),
          )),
      )
      .await;

      request.send_request(&app).await
    }
  }

  #[actix_web::test]
  async fn get_http_tickets() {
    server_tests::test_get(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn post_http_tickets() {
    server_tests::test_post(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn parameterized_get_http_tickets() {
    server_tests::test_parameterized_get(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn parameterized_post_http_tickets() {
    server_tests::test_parameterized_post(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn parameterized_post_class_header_http_tickets() {
    server_tests::test_parameterized_post_class_header(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn service_info() {
    server_tests::test_service_info(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn get_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_get(&ActixTestServer::new_with_tls(base_path.path())).await;
  }

  #[actix_web::test]
  async fn post_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_post(&ActixTestServer::new_with_tls(base_path.path())).await;
  }

  #[actix_web::test]
  async fn parameterized_get_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_parameterized_get(&ActixTestServer::new_with_tls(base_path.path())).await;
  }

  #[actix_web::test]
  async fn parameterized_post_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_parameterized_post(&ActixTestServer::new_with_tls(base_path.path())).await;
  }

  #[actix_web::test]
  async fn parameterized_post_class_header_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_parameterized_post_class_header(&ActixTestServer::new_with_tls(
      base_path.path(),
    ))
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
