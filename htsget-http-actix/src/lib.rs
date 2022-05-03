use std::net::SocketAddr;
use std::sync::Arc;

use actix_web::{App, HttpServer, web};
use actix_web::dev::Server;

use htsget_config::config::{Config, ConfigServiceInfo, StorageType};
use htsget_config::regex_resolver::RegexResolver;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::htsget::HtsGet;
use htsget_search::storage::aws::AwsS3Storage;
use htsget_search::storage::axum_server::HttpsFormatter;
use htsget_search::storage::local::LocalStorage;
use htsget_search::storage::UrlFormatter;

use crate::handlers::{get, post, reads_service_info, variants_service_info};

pub mod handlers;

pub type HtsGetStorage<T> = HtsGetFromStorage<LocalStorage<T>>;

pub struct AppState<H: HtsGet> {
  pub htsget: Arc<H>,
  pub config_service_info: ConfigServiceInfo,
}

pub fn configure_server<H: HtsGet + Send + Sync + 'static>(
  service_config: &mut web::ServiceConfig,
  htsget: H,
  config_service_info: ConfigServiceInfo
) {
  service_config
    .app_data(web::Data::new(AppState {
      htsget: Arc::new(htsget),
      config_service_info
    }))
    .service(
      web::scope("/reads")
        .route(
          "/service-info",
          web::get().to(reads_service_info::<H>),
        )
        .route(
          "/service-info",
          web::post().to(reads_service_info::<H>),
        )
        .route("/{id:.+}", web::get().to(get::reads::<H>))
        .route("/{id:.+}", web::post().to(post::reads::<H>)),
    )
    .service(
      web::scope("/variants")
        .route(
          "/service-info",
          web::get().to(variants_service_info::<H>),
        )
        .route(
          "/service-info",
          web::post().to(variants_service_info::<H>),
        )
        .route("/{id:.+}", web::get().to(get::variants::<H>))
        .route(
          "/{id:.+}",
          web::post().to(post::variants::<H>),
        ),
    );
}

pub fn run_server<H: HtsGet + Clone + Send + Sync + 'static>(htsget: H, config_service_info: ConfigServiceInfo, addr: SocketAddr) -> std::io::Result<Server> {
  Ok(HttpServer::new(Box::new(move || {
    App::new().configure(|service_config: &mut web::ServiceConfig| {
      configure_server(
        service_config,
        htsget.clone(),
        config_service_info.clone()
      );
    })
  })).bind(addr)?.run())
}

#[cfg(test)]
mod tests {
  use actix_web::{App, test, web};
  use actix_web::web::Bytes;
  use async_trait::async_trait;

  use htsget_search::storage::axum_server::HttpsFormatter;
  use htsget_test_utils::{
    Header as TestHeader, Response as TestResponse, server_tests, TestRequest, TestServer,
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
          .method(method.into().parse().expect("Expected valid method.")),
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
      let config = self.get_config();
      let app = test::init_service(App::new().configure(
        |service_config: &mut web::ServiceConfig| {
          configure_server(
            service_config,
            HtsGetFromStorage::new(
              LocalStorage::new(
                self.config.htsget_path.clone(),
                self.config.htsget_resolver.clone(),
                HttpsFormatter::from(self.config.htsget_addr)
              ).unwrap(),
            ),
            self.config.htsget_config_service_info.clone()
          );
        },
      ))
      .await;
      let response = request.0.send_request(&app).await;
      let status: u16 = response.status().into();
      let bytes: Bytes = test::read_body(response).await;
      TestResponse::new(status, bytes)
    }
  }

  #[actix_web::test]
  async fn test_get() {
    server_tests::test_get(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn test_post() {
    server_tests::test_post(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn test_parameterized_get() {
    server_tests::test_parameterized_get(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn test_parameterized_post() {
    server_tests::test_parameterized_post(&ActixTestServer::default()).await;
  }

  #[actix_web::test]
  async fn test_service_info() {
    server_tests::test_service_info(&ActixTestServer::default()).await;
  }
}
