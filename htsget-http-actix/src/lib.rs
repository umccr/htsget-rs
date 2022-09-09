use std::net::SocketAddr;
use std::sync::Arc;

use actix_cors::Cors;
use actix_web::dev::Server;
use actix_web::http::Method;
use actix_web::{web, App, HttpServer};
use tracing::info;
use tracing::instrument;
use tracing_actix_web::TracingLogger;

use htsget_config::config::ServiceInfo;
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
pub fn configure_cors(cors_allow_credentials: bool) -> Cors {
  let cors = Cors::default()
    .allow_any_origin()
    .allowed_methods(vec![Method::GET])
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
  config_service_info: ServiceInfo,
  cors_allow_credentials: bool,
  addr: SocketAddr,
) -> std::io::Result<Server> {
  let server = HttpServer::new(Box::new(move || {
    App::new()
      .configure(|service_config: &mut web::ServiceConfig| {
        configure_server(service_config, htsget.clone(), config_service_info.clone());
      })
      .wrap(configure_cors(cors_allow_credentials))
      .wrap(TracingLogger::default())
  }))
  .bind(addr)?;

  info!(addresses = ?server.addrs(), "htsget query server addresses bound");
  Ok(server.run())
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use actix_web::{test, web, App};
  use async_trait::async_trait;
  use tempfile::TempDir;

  use htsget_config::config::Config;
  use htsget_test_utils::server_tests;
  use htsget_test_utils::server_tests::{
    config_with_tls, formatter_and_expected_path, Header as TestHeader, Response as TestResponse,
    TestRequest, TestServer,
  };

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
        config: server_tests::default_test_config(),
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

      let app = test::init_service(App::new().configure(
        |service_config: &mut web::ServiceConfig| {
          configure_server(
            service_config,
            HtsGetFromStorage::local_from(
              self.config.path.clone(),
              self.config.resolver.clone(),
              formatter,
            )
            .unwrap(),
            self.config.service_info.clone(),
          );
        },
      ))
      .await;
      let response = request.0.send_request(&app).await;
      let status: u16 = response.status().into();
      let bytes = test::read_body(response).await.to_vec();

      TestResponse::new(status, bytes, expected_path)
    }
  }

  impl ActixTestServer {
    fn new_with_tls<P: AsRef<Path>>(path: P) -> Self {
      Self {
        config: config_with_tls(path),
      }
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
}
