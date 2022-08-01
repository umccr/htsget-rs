//! Library providing the routing and http responses for aws lambda requests.
//!

use std::collections::HashMap;
use std::sync::Arc;

use lambda_http::{Body, http, Request, Response};
use lambda_http::ext::RequestExt;
use lambda_http::http::{Method, StatusCode, Uri};
use tracing::debug;

use htsget_config::config::ServiceInfo;
use htsget_http_core::{Endpoint, PostRequest};
use htsget_search::htsget::HtsGet;

use crate::handlers::get::get;
use crate::handlers::post::post;
use crate::handlers::service_info::get_service_info_json;

pub mod handlers;

/// A request route, with a method, endpoint and route type.
#[derive(Debug, PartialEq)]
pub struct Route {
  method: HtsgetMethod,
  endpoint: Endpoint,
  route_type: RouteType,
}

/// Valid htsget http request methods.
#[derive(Debug, PartialEq)]
pub enum HtsgetMethod {
  Get,
  Post,
}

/// A route type, which is either the service info endpoint, or an id represented by a string.
#[derive(Debug, PartialEq)]
pub enum RouteType {
  ServiceInfo,
  Id(String),
}

impl Route {
  pub fn new(method: HtsgetMethod, endpoint: Endpoint, route_type: RouteType) -> Self {
    Self {
      method,
      endpoint,
      route_type,
    }
  }
}

/// A Router is a struct which handles routing any htsget requests to the htsget search, using the config.
pub struct Router<'a, H> {
  searcher: Arc<H>,
  config_service_info: &'a ServiceInfo,
}

impl<'a, H: HtsGet + Send + Sync + 'static> Router<'a, H> {
  pub fn new(searcher: Arc<H>, config_service_info: &'a ServiceInfo) -> Self {
    Self {
      searcher,
      config_service_info,
    }
  }

  /// Gets the Route if the request is valid, otherwise returns None.
  fn get_route(&self, method: &Method, uri: &Uri) -> Option<Route> {
    let with_endpoint = |endpoint: Endpoint, endpoint_type: &str| {
      if endpoint_type.is_empty() {
        None
      } else {
        let method = match *method {
          Method::GET => Some(HtsgetMethod::Get),
          Method::POST => Some(HtsgetMethod::Post),
          _ => None,
        }?;
        if endpoint_type == "service-info" {
          Some(Route::new(method, endpoint, RouteType::ServiceInfo))
        } else {
          Some(Route::new(
            method,
            endpoint,
            RouteType::Id(endpoint_type.to_string()),
          ))
        }
      }
    };

    uri.path().strip_prefix("/reads/").map_or_else(
      || {
        uri
          .path()
          .strip_prefix("/variants/")
          .and_then(|variants| with_endpoint(Endpoint::Variants, variants))
      },
      |reads| with_endpoint(Endpoint::Reads, reads),
    )
  }

  /// Routes the request to the relevant htsget search endpoint using the lambda request, returning a http response.
  pub async fn route_request(&self, request: Request) -> http::Result<Response<Body>> {
    match self.get_route(request.method(), &request.raw_http_path().parse::<Uri>()?) {
      Some(Route {
        endpoint,
        route_type: RouteType::ServiceInfo,
        ..
      }) => get_service_info_json(self.searcher.clone(), endpoint, self.config_service_info),
      Some(Route {
        method: HtsgetMethod::Get,
        endpoint,
        route_type: RouteType::Id(id),
      }) => {
        get(
          id,
          self.searcher.clone(),
          Self::extract_query(&request),
          endpoint,
        )
        .await
      }
      Some(Route {
        method: HtsgetMethod::Post,
        endpoint,
        route_type: RouteType::Id(id),
      }) => match Self::extract_query_from_payload(&request) {
        None => Ok(
          Response::builder()
            .status(StatusCode::UNSUPPORTED_MEDIA_TYPE)
            .body(Body::Empty)?,
        ),
        Some(query) => post(id, self.searcher.clone(), query, endpoint).await,
      },
      _ => Ok(
        Response::builder()
          .status(StatusCode::METHOD_NOT_ALLOWED)
          .body(Body::Empty)?,
      ),
    }
  }

  /// Extracts post request query parameters.
  fn extract_query_from_payload(request: &Request) -> Option<PostRequest> {
    if request.body().is_empty() {
      Some(PostRequest::default())
    } else {
      let payload = request.payload::<PostRequest>();
      debug!(payload = ?payload, "POST request payload");
      // Allows null/empty bodies.
      payload.ok()?
    }
  }

  /// Extract get request query parameters.
  fn extract_query(request: &Request) -> HashMap<String, String> {
    let mut query = HashMap::new();
    // Silently ignores all but the last query key, for keys that are present more than once.
    // This is the way actix-web does it, but should we return an error instead if a key is present
    // more than once?
    for (key, value) in request.query_string_parameters().iter() {
      query.insert(key.to_string(), value.to_string());
    }
    debug!(query = ?query, "GET request query");
    query
  }
}

#[cfg(test)]
mod tests {
  use std::future::Future;
  use std::path::Path;
  use std::str::FromStr;
  use std::sync::Arc;

  use async_trait::async_trait;
  use lambda_http::{Request, RequestExt};
  use lambda_http::Body::Text;
  use lambda_http::http::header::HeaderName;
  use lambda_http::http::Uri;
  use query_map::QueryMap;
  use tempfile::TempDir;

  use htsget_config::config::Config;
  use htsget_http_core::Endpoint;
  use htsget_search::htsget::{Class, HtsGet};
  use htsget_search::htsget::from_storage::HtsGetFromStorage;
  use htsget_search::storage::local::LocalStorage;
  use htsget_search::storage::ticket_server::HttpTicketFormatter;
  use htsget_test_utils::server_tests;
  use htsget_test_utils::server_tests::{
    config_with_tls, default_test_config, expected_url_path, formatter_and_expected_path,
    formatter_from_config, get_test_file, Header, Response, test_response,
    test_response_service_info, TestRequest, TestServer,
  };

  use crate::{HtsgetMethod, Method, Route, Router, RouteType};

  struct LambdaTestServer {
    config: Config,
  }

  struct LambdaTestRequest<T>(T);

  impl TestRequest for LambdaTestRequest<Request> {
    fn insert_header(mut self, header: Header<impl Into<String>>) -> Self {
      self.0.headers_mut().insert(
        HeaderName::from_str(&header.name.into()).expect("Expected valid header name."),
        header
          .value
          .into()
          .parse()
          .expect("Expected valid header value."),
      );
      self
    }

    fn set_payload(mut self, payload: impl Into<String>) -> Self {
      *self.0.body_mut() = Text(payload.into());
      self
    }

    fn uri(mut self, uri: impl Into<String>) -> Self {
      let uri = uri.into();
      *self.0.uri_mut() = uri.parse().expect("Expected valid uri.");
      if let Some(query) = self.0.uri().query().map(|s| s.to_string()) {
        Self(
          self
            .0
            .with_query_string_parameters(
              query
                .parse::<QueryMap>()
                .expect("Expected valid query parameters."),
            )
            .with_raw_http_path(&uri),
        )
      } else {
        Self(self.0.with_raw_http_path(&uri))
      }
    }

    fn method(mut self, method: impl Into<String>) -> Self {
      *self.0.method_mut() = method.into().parse().expect("Expected valid method.");
      self
    }
  }

  impl Default for LambdaTestServer {
    fn default() -> Self {
      Self {
        config: default_test_config(),
      }
    }
  }

  #[async_trait(?Send)]
  impl TestServer<LambdaTestRequest<Request>> for LambdaTestServer {
    fn get_config(&self) -> &Config {
      &self.config
    }

    fn get_request(&self) -> LambdaTestRequest<Request> {
      LambdaTestRequest(Request::default())
    }

    async fn test_server(&self, request: LambdaTestRequest<Request>) -> Response {
      let (expected_path, formatter) = formatter_and_expected_path(self.get_config()).await;

      let router = Router::new(
        Arc::new(
          HtsGetFromStorage::local_from(&self.config.path, self.config.resolver.clone(), formatter)
            .unwrap(),
        ),
        &self.config.service_info,
      );

      route_request_to_response(request.0, router, expected_path).await
    }
  }

  impl LambdaTestServer {
    fn new_with_tls<P: AsRef<Path>>(path: P) -> Self {
      Self {
        config: config_with_tls(path),
      }
    }
  }

  #[tokio::test]
  async fn get_http_tickets() {
    server_tests::test_get(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn post_http_tickets() {
    server_tests::test_post(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn parameterized_get_http_tickets() {
    server_tests::test_parameterized_get(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn parameterized_post_http_tickets() {
    server_tests::test_parameterized_post(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn parameterized_post_class_header_http_tickets() {
    server_tests::test_parameterized_post_class_header(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn get_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_get(&LambdaTestServer::new_with_tls(base_path.path())).await;
  }

  #[tokio::test]
  async fn post_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_post(&LambdaTestServer::new_with_tls(base_path.path())).await;
  }

  #[tokio::test]
  async fn parameterized_get_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_parameterized_get(&LambdaTestServer::new_with_tls(base_path.path())).await;
  }

  #[tokio::test]
  async fn parameterized_post_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_parameterized_post(&LambdaTestServer::new_with_tls(base_path.path())).await;
  }

  #[tokio::test]
  async fn parameterized_post_class_header_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_parameterized_post_class_header(&LambdaTestServer::new_with_tls(
      base_path.path(),
    ))
    .await;
  }

  #[tokio::test]
  async fn service_info() {
    server_tests::test_service_info(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn get_from_file_http_tickets() {
    let config = default_test_config();
    endpoint_from_file("events/event_get.json", Class::Body, &config).await;
  }

  #[tokio::test]
  async fn post_from_file_http_tickets() {
    let config = default_test_config();
    endpoint_from_file("events/event_post.json", Class::Body, &config).await;
  }

  #[tokio::test]
  async fn parameterized_get_from_file_http_tickets() {
    let config = default_test_config();
    endpoint_from_file(
      "events/event_parameterized_get.json",
      Class::Header,
      &config,
    )
    .await;
  }

  #[tokio::test]
  async fn parameterized_post_from_file_http_tickets() {
    let config = default_test_config();
    endpoint_from_file("events/event_parameterized_post.json", Class::Body, &config).await;
  }

  #[tokio::test]
  async fn parameterized_post_class_header_from_file_http_tickets() {
    let config = default_test_config();
    endpoint_from_file(
      "events/event_parameterized_post_class_header.json",
      Class::Header,
      &config,
    )
    .await;
  }

  #[tokio::test]
  async fn get_from_file_https_tickets() {
    let base_path = TempDir::new().unwrap();
    let config = config_with_tls(base_path.path());
    endpoint_from_file("events/event_get.json", Class::Body, &config).await;
  }

  #[tokio::test]
  async fn post_from_file_https_tickets() {
    let base_path = TempDir::new().unwrap();
    let config = config_with_tls(base_path.path());
    endpoint_from_file("events/event_post.json", Class::Body, &config).await;
  }

  #[tokio::test]
  async fn parameterized_get_from_file_https_tickets() {
    let base_path = TempDir::new().unwrap();
    let config = config_with_tls(base_path.path());
    endpoint_from_file(
      "events/event_parameterized_get.json",
      Class::Header,
      &config,
    )
    .await;
  }

  #[tokio::test]
  async fn parameterized_post_from_file_https_tickets() {
    let base_path = TempDir::new().unwrap();
    let config = config_with_tls(base_path.path());
    endpoint_from_file("events/event_parameterized_post.json", Class::Body, &config).await;
  }

  #[tokio::test]
  async fn parameterized_post_class_header_from_file_https_tickets() {
    let base_path = TempDir::new().unwrap();
    let config = config_with_tls(base_path.path());
    endpoint_from_file(
      "events/event_parameterized_post_class_header.json",
      Class::Header,
      &config,
    )
    .await;
  }

  #[tokio::test]
  async fn service_info_from_file() {
    let config = default_test_config();
    test_service_info_from_file("events/event_service_info.json", &config).await;
  }

  #[tokio::test]
  async fn get_route_invalid_method() {
    let config = default_test_config();
    with_router(
      |router| async move {
        let uri = Uri::builder().path_and_query("/reads/id").build().unwrap();
        assert!(router.get_route(&Method::DELETE, &uri).is_none());
      },
      &config,
      formatter_from_config(&config),
    )
    .await;
  }

  #[tokio::test]
  async fn get_route_no_path() {
    let config = default_test_config();
    with_router(
      |router| async move {
        let uri = Uri::builder().path_and_query("").build().unwrap();
        assert!(router.get_route(&Method::GET, &uri).is_none());
      },
      &config,
      formatter_from_config(&config),
    )
    .await;
  }

  #[tokio::test]
  async fn get_route_no_endpoint() {
    let config = default_test_config();
    with_router(
      |router| async move {
        let uri = Uri::builder().path_and_query("/path/").build().unwrap();
        assert!(router.get_route(&Method::GET, &uri).is_none());
      },
      &config,
      formatter_from_config(&config),
    )
    .await;
  }

  #[tokio::test]
  async fn get_route_reads_no_id() {
    let config = default_test_config();
    with_router(
      |router| async move {
        let uri = Uri::builder().path_and_query("/reads/").build().unwrap();
        assert!(router.get_route(&Method::GET, &uri).is_none());
      },
      &config,
      formatter_from_config(&config),
    )
    .await;
  }

  #[tokio::test]
  async fn get_route_variants_no_id() {
    let config = default_test_config();
    with_router(
      |router| async move {
        let uri = Uri::builder().path_and_query("/variants/").build().unwrap();
        assert!(router.get_route(&Method::GET, &uri).is_none());
      },
      &config,
      formatter_from_config(&config),
    )
    .await;
  }

  #[tokio::test]
  async fn get_route_reads_service_info() {
    let config = default_test_config();
    with_router(
      |router| async move {
        let uri = Uri::builder()
          .path_and_query("/reads/service-info")
          .build()
          .unwrap();
        let route = router.get_route(&Method::GET, &uri);
        assert_eq!(
          route,
          Some(Route {
            method: HtsgetMethod::Get,
            endpoint: Endpoint::Reads,
            route_type: RouteType::ServiceInfo
          })
        );
      },
      &config,
      formatter_from_config(&config),
    )
    .await;
  }

  #[tokio::test]
  async fn get_route_variants_service_info() {
    let config = default_test_config();
    with_router(
      |router| async move {
        let uri = Uri::builder()
          .path_and_query("/variants/service-info")
          .build()
          .unwrap();
        let route = router.get_route(&Method::GET, &uri);
        assert_eq!(
          route,
          Some(Route {
            method: HtsgetMethod::Get,
            endpoint: Endpoint::Variants,
            route_type: RouteType::ServiceInfo
          })
        );
      },
      &config,
      formatter_from_config(&config),
    )
    .await;
  }

  #[tokio::test]
  async fn get_route_reads_id() {
    let config = default_test_config();
    with_router(
      |router| async move {
        let uri = Uri::builder().path_and_query("/reads/id").build().unwrap();
        let route = router.get_route(&Method::GET, &uri);
        assert_eq!(
          route,
          Some(Route {
            method: HtsgetMethod::Get,
            endpoint: Endpoint::Reads,
            route_type: RouteType::Id("id".to_string())
          })
        );
      },
      &config,
      formatter_from_config(&config),
    )
    .await;
  }

  #[tokio::test]
  async fn get_route_variants_id() {
    let config = default_test_config();
    with_router(
      |router| async move {
        let uri = Uri::builder()
          .path_and_query("/variants/id")
          .build()
          .unwrap();
        let route = router.get_route(&Method::GET, &uri);
        assert_eq!(
          route,
          Some(Route {
            method: HtsgetMethod::Get,
            endpoint: Endpoint::Variants,
            route_type: RouteType::Id("id".to_string())
          })
        );
      },
      &config,
      formatter_from_config(&config),
    )
    .await;
  }

  async fn with_router<'a, F, Fut>(test: F, config: &'a Config, formatter: HttpTicketFormatter)
  where
    F: FnOnce(Router<'a, HtsGetFromStorage<LocalStorage<HttpTicketFormatter>>>) -> Fut,
    Fut: Future<Output = ()>,
  {
    let router = Router::new(
      Arc::new(
        HtsGetFromStorage::local_from(&config.path, config.resolver.clone(), formatter).unwrap(),
      ),
      &config.service_info,
    );
    test(router).await;
  }

  fn get_request_from_file(file_path: &str) -> Request {
    let event = get_test_file(file_path);
    lambda_http::request::from_str(&event).expect("Failed to create lambda request.")
  }

  async fn endpoint_from_file(file_path: &str, class: Class, config: &Config) {
    let (expected_path, formatter) = formatter_and_expected_path(config).await;
    with_router(
      |router| async move {
        let response =
          route_request_to_response(get_request_from_file(file_path), router, expected_path).await;
        test_response(response, class).await;
      },
      config,
      formatter,
    )
    .await;
  }

  async fn test_service_info_from_file(file_path: &str, config: &Config) {
    let formatter = formatter_from_config(config);
    let expected_path = expected_url_path(&formatter);
    with_router(
      |router| async {
        let response =
          route_request_to_response(get_request_from_file(file_path), router, expected_path).await;
        test_response_service_info(&response);
      },
      config,
      formatter,
    )
    .await;
  }

  async fn route_request_to_response<T: HtsGet + Send + Sync + 'static>(
    request: Request,
    router: Router<'_, T>,
    expected_path: String,
  ) -> Response {
    let response = router
      .route_request(request)
      .await
      .expect("Failed to route request.");
    let status: u16 = response.status().into();
    let body = response.body().to_vec();
    Response::new(status, body, expected_path)
  }
}
