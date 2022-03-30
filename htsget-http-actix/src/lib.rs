#[cfg(feature = "async")]
use std::sync::Arc;

use actix_web::web;
use htsget_config::config::HtsgetConfig;
use htsget_config::data_sources::RegexResolver;

// Async
#[cfg(feature = "async")]
use crate::handlers::{get, post, reads_service_info, variants_service_info};
#[cfg(feature = "async")]
use htsget_search::htsget::from_storage::HtsGetFromStorage;
#[cfg(feature = "async")]
use htsget_search::htsget::HtsGet;

// Blocking
#[cfg(not(feature = "async"))]
use crate::handlers::blocking::{get, post, reads_service_info, variants_service_info};
#[cfg(not(feature = "async"))]
use htsget_search::htsget::blocking::from_storage::HtsGetFromStorage;
#[cfg(not(feature = "async"))]
use htsget_search::htsget::blocking::HtsGet;

#[cfg(not(feature = "async"))]
use htsget_search::storage::blocking::local::LocalStorage;
#[cfg(feature = "async")]
use htsget_search::storage::local::LocalStorage;

pub mod handlers;

#[cfg(feature = "async")]
pub type AsyncHtsGetStorage = HtsGetFromStorage<LocalStorage>;
#[cfg(not(feature = "async"))]
pub type HtsGetStorage = HtsGetFromStorage<LocalStorage>;

#[cfg(feature = "async")]
pub struct AsyncAppState<H: HtsGet> {
  pub htsget: Arc<H>,
  pub config: HtsgetConfig,
}

#[cfg(not(feature = "async"))]
pub struct AppState<H: HtsGet> {
  pub htsget: H,
  pub config: HtsgetConfig,
}

#[cfg(feature = "async")]
pub fn async_configure_server(service_config: &mut web::ServiceConfig, config: HtsgetConfig) {
  let htsget_path = config.htsget_path.clone();
  let regex_match = config.htsget_regex_match.clone();
  let regex_substitution = config.htsget_regex_substitution.clone();
  service_config
    .app_data(web::Data::new(AsyncAppState {
      htsget: Arc::new(AsyncHtsGetStorage::new(
        LocalStorage::new(
          htsget_path,
          RegexResolver::new(&regex_match, &regex_substitution).unwrap(),
        )
        .expect("Couldn't create a Storage with the provided path"),
      )),
      config,
    }))
    .service(
      web::scope("/reads")
        .route(
          "/service-info",
          web::get().to(reads_service_info::<AsyncHtsGetStorage>),
        )
        .route(
          "/service-info",
          web::post().to(reads_service_info::<AsyncHtsGetStorage>),
        )
        .route("/{id:.+}", web::get().to(get::reads::<AsyncHtsGetStorage>))
        .route(
          "/{id:.+}",
          web::post().to(post::reads::<AsyncHtsGetStorage>),
        ),
    )
    .service(
      web::scope("/variants")
        .route(
          "/service-info",
          web::get().to(variants_service_info::<AsyncHtsGetStorage>),
        )
        .route(
          "/service-info",
          web::post().to(variants_service_info::<AsyncHtsGetStorage>),
        )
        .route(
          "/{id:.+}",
          web::get().to(get::variants::<AsyncHtsGetStorage>),
        )
        .route(
          "/{id:.+}",
          web::post().to(post::variants::<AsyncHtsGetStorage>),
        ),
    );
}

#[cfg(not(feature = "async"))]
pub fn configure_server(service_config: &mut web::ServiceConfig, config: HtsgetConfig) {
  let htsget_path = config.htsget_path.clone();
  let regex_match = config.htsget_regex_match.clone();
  let regex_substitution = config.htsget_regex_substitution.clone();
  service_config
    .app_data(web::Data::new(AppState {
      htsget: HtsGetStorage::new(
        LocalStorage::new(
          htsget_path,
          RegexResolver::new(&regex_match, &regex_substitution).unwrap(),
        )
        .expect("Couldn't create a Storage with the provided path"),
      ),
      config,
    }))
    .service(
      web::scope("/reads")
        .route(
          "/service-info",
          web::get().to(reads_service_info::<HtsGetStorage>),
        )
        .route(
          "/service-info",
          web::post().to(reads_service_info::<HtsGetStorage>),
        )
        .route("/{id:.+}", web::get().to(get::reads::<HtsGetStorage>))
        .route("/{id:.+}", web::post().to(post::reads::<HtsGetStorage>)),
    )
    .service(
      web::scope("/variants")
        .route(
          "/service-info",
          web::get().to(variants_service_info::<HtsGetStorage>),
        )
        .route(
          "/service-info",
          web::post().to(variants_service_info::<HtsGetStorage>),
        )
        .route("/{id:.+}", web::get().to(get::variants::<HtsGetStorage>))
        .route("/{id:.+}", web::post().to(post::variants::<HtsGetStorage>)),
    );
}

#[cfg(test)]
mod tests {
  #[cfg(feature = "async")]
  use super::async_configure_server as configure_server;
  #[cfg(not(feature = "async"))]
  use super::configure_server;
  use super::*;

  use actix_web::http::StatusCode;
  use actix_web::test::TestRequest;
  use actix_web::{test, web, App};
  use htsget_http_core::get_service_info_with;
  use htsget_http_core::{Endpoint, JsonResponse, ServiceInfo};
  use htsget_search::htsget::Class::Header;
  use htsget_search::htsget::{Format, Headers, Response, Url};
  use serde::Deserialize;
  use std::collections::HashMap;
  use std::path::{Path, PathBuf};

  #[actix_web::test]
  async fn test_get() {
    let request = test::TestRequest::get().uri("/variants/data/vcf/sample1-bcbio-cancer");

    with_response(request, |path, status, response: JsonResponse| {
      assert!(status.is_success());
      assert_eq!(example_response(&path), response);
    })
    .await;
  }

  #[actix_web::test]
  async fn test_post() {
    let request = test::TestRequest::post()
      .insert_header(("content-type", "application/json"))
      .set_payload("{}")
      .uri("/variants/data/vcf/sample1-bcbio-cancer");

    with_response(request, |path, status, response: JsonResponse| {
      assert!(status.is_success());
      assert_eq!(example_response(&path), response);
    })
    .await;
  }

  #[actix_web::test]
  async fn test_parameterized_get() {
    let request = test::TestRequest::get()
      .uri("/variants/data/vcf/sample1-bcbio-cancer?format=VCF&class=header");

    with_response(request, |path, status, response: JsonResponse| {
      assert!(status.is_success());
      assert_eq!(example_response_header(&path), response);
    })
    .await;
  }

  #[actix_web::test]
  async fn test_parameterized_post() {
    let request = test::TestRequest::post()
      .insert_header(("content-type", "application/json"))
      .set_payload("{\"format\": \"VCF\", \"regions\": [{\"referenceName\": \"chrM\"}]}")
      .uri("/variants/data/vcf/sample1-bcbio-cancer");

    with_response(request, |path, status, response: JsonResponse| {
      assert!(status.is_success());
      assert_eq!(example_response(&path), response);
    })
    .await;
  }

  #[actix_web::test]
  async fn test_service_info() {
    let request = test::TestRequest::get().uri("/variants/service-info");

    with_response(request, |_, status, response: ServiceInfo| {
      let expected = get_service_info_with(
        Endpoint::Variants,
        &[Format::Vcf, Format::Bcf],
        false,
        false,
      );

      assert!(status.is_success());
      assert_eq!(expected, response);
    })
    .await;
  }

  fn example_response(path: &Path) -> JsonResponse {
    let mut headers = HashMap::new();
    headers.insert("Range".to_string(), "bytes=0-3367".to_string());
    JsonResponse::from_response(Response::new(
      Format::Vcf,
      vec![Url::new(format!(
        "file://{}",
        path
          .join("data")
          .join("vcf")
          .join("sample1-bcbio-cancer.vcf.gz")
          .to_string_lossy()
      ))
      .with_headers(Headers::new(headers))],
    ))
  }

  fn example_response_header(path: &Path) -> JsonResponse {
    let mut headers = HashMap::new();
    headers.insert("Range".to_string(), "bytes=0-3367".to_string());
    JsonResponse::from_response(Response::new(
      Format::Vcf,
      vec![Url::new(format!(
        "file://{}",
        path
          .join("data")
          .join("vcf")
          .join("sample1-bcbio-cancer.vcf.gz")
          .to_string_lossy()
      ))
      .with_headers(Headers::new(headers))
      .with_class(Header)],
    ))
  }

  async fn with_response<F, T>(request: TestRequest, test: F)
  where
    T: for<'de> Deserialize<'de>,
    F: FnOnce(PathBuf, StatusCode, T),
  {
    std::env::set_var(
      "HTSGET_PATH",
      PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    );

    let config =
      envy::from_env::<HtsgetConfig>().expect("The environment variables weren't properly set!");
    let app = test::init_service(App::new().configure(
      |service_config: &mut web::ServiceConfig| {
        configure_server(service_config, config.clone());
      },
    ))
    .await;
    let response = request.send_request(&app).await;
    let status = response.status();
    let response_json = test::read_body_json(response).await;

    test(config.htsget_path, status, response_json);
  }
}
