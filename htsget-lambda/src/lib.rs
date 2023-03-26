//! Library providing the routing and http responses for aws lambda requests.
//!

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use lambda_http::ext::RequestExt;
use lambda_http::http::{Method, StatusCode, Uri};
use lambda_http::tower::ServiceBuilder;
use lambda_http::{http, service_fn, Body, Request, Response};
use lambda_runtime::Error;
use tracing::instrument;
use tracing::{debug, info};

use htsget_config::config::cors::CorsConfig;
pub use htsget_config::config::{Config, DataServerConfig, ServiceInfo, TicketServerConfig};
pub use htsget_config::storage::Storage;
use htsget_http::{Endpoint, PostRequest};
use htsget_search::htsget::HtsGet;
use htsget_search::storage::configure_cors;

use crate::handlers::get::get;
use crate::handlers::post::post;
use crate::handlers::service_info::get_service_info_json;

pub mod handlers;

/// A request route, with a method, endpoint and route type.
#[derive(Debug, PartialEq, Eq)]
pub struct Route {
  method: HtsgetMethod,
  endpoint: Endpoint,
  route_type: RouteType,
}

/// Valid htsget http request methods.
#[derive(Debug, PartialEq, Eq)]
pub enum HtsgetMethod {
  Get,
  Post,
}

/// A route type, which is either the service info endpoint, or an id represented by a string.
#[derive(Debug, PartialEq, Eq)]
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

  /// Gets the Route if the request is valid, otherwise returns an error with a response.
  fn get_route(method: &Method, uri: &Uri) -> Result<Self, http::Result<Response<Body>>> {
    let with_endpoint = |endpoint: Endpoint, endpoint_type: &str| {
      if endpoint_type.is_empty() {
        Err(
          Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::Empty),
        )
      } else {
        let method = match *method {
          Method::GET => Ok(HtsgetMethod::Get),
          Method::POST => Ok(HtsgetMethod::Post),
          _ => Err(
            Response::builder()
              .status(StatusCode::METHOD_NOT_ALLOWED)
              .body(Body::Empty),
          ),
        }?;
        if endpoint_type == "service-info" {
          Ok(Route::new(method, endpoint, RouteType::ServiceInfo))
        } else {
          Ok(Route::new(
            method,
            endpoint,
            RouteType::Id(endpoint_type.to_string()),
          ))
        }
      }
    };

    uri.path().strip_prefix("/reads/").map_or_else(
      || {
        uri.path().strip_prefix("/variants/").map_or_else(
          || {
            Err(
              Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::Empty),
            )
          },
          |variants| with_endpoint(Endpoint::Variants, variants),
        )
      },
      |reads| with_endpoint(Endpoint::Reads, reads),
    )
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

  /// Routes the request to the relevant htsget search endpoint using the lambda request, returning a http response.
  pub async fn route_request(&self, request: Request) -> http::Result<Response<Body>> {
    match Route::get_route(request.method(), &request.raw_http_path().parse::<Uri>()?) {
      Ok(Route {
        endpoint,
        route_type: RouteType::ServiceInfo,
        ..
      }) => get_service_info_json(self.searcher.clone(), endpoint, self.config_service_info),
      Ok(Route {
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
      Ok(Route {
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
      Err(err) => err,
    }
  }

  /// Extracts post request query parameters.
  #[instrument(level = "debug", ret)]
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
  #[instrument(level = "debug", ret)]
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

pub async fn handle_request_service_fn<F, Fut>(cors: CorsConfig, service: F) -> Result<(), Error>
where
  F: FnMut(Request) -> Fut,
  Fut: Future<Output = http::Result<Response<Body>>> + Send,
{
  let cors_layer = configure_cors(cors)?;

  let handler = ServiceBuilder::new()
    .layer(cors_layer)
    .service(service_fn(service));

  lambda_http::run(handler).await?;

  Ok(())
}

pub async fn handle_request<H>(cors: CorsConfig, router: &Router<'_, H>) -> Result<(), Error>
where
  H: HtsGet + Send + Sync + 'static,
{
  handle_request_service_fn(cors, |event: Request| async move {
    info!(event = ?event, "received request");
    router.route_request(event).await
  })
  .await
}

#[cfg(test)]
mod tests {
  use std::future::Future;
  use std::path::Path;
  use std::str::FromStr;
  use std::sync::Arc;

  use async_trait::async_trait;
  use lambda_http::http::header::HeaderName;
  use lambda_http::http::Uri;
  use lambda_http::tower::ServiceExt;
  use lambda_http::Body::Text;
  use lambda_http::{Request, RequestExt, Service};
  use query_map::QueryMap;
  use tempfile::TempDir;

  use htsget_config::resolver::Resolver;
  use htsget_config::types::{Class, JsonResponse};
  use htsget_http::Endpoint;
  use htsget_search::storage::configure_cors;
  use htsget_search::storage::data_server::BindDataServer;
  use htsget_test::http_tests::{config_with_tls, default_test_config, get_test_file};
  use htsget_test::http_tests::{Header, Response as TestResponse, TestRequest, TestServer};
  use htsget_test::server_tests::{expected_url_path, test_response, test_response_service_info};
  use htsget_test::{cors_tests, server_tests};

  use super::*;

  struct LambdaTestServer {
    config: Config,
  }

  struct LambdaTestRequest<T>(T);

  impl TestRequest for LambdaTestRequest<Request> {
    fn insert_header(mut self, header: Header<impl Into<String>>) -> Self {
      self.0.headers_mut().insert(
        HeaderName::from_str(&header.name.into()).expect("expected valid header name"),
        header
          .value
          .into()
          .parse()
          .expect("expected valid header value"),
      );
      self
    }

    fn set_payload(mut self, payload: impl Into<String>) -> Self {
      *self.0.body_mut() = Text(payload.into());
      self
    }

    fn uri(mut self, uri: impl Into<String>) -> Self {
      let uri = uri.into();
      *self.0.uri_mut() = uri.parse().expect("expected valid uri");
      if let Some(query) = self.0.uri().query().map(|s| s.to_string()) {
        Self(
          self
            .0
            .with_query_string_parameters(
              query
                .parse::<QueryMap>()
                .expect("expected valid query parameters"),
            )
            .with_raw_http_path(&uri),
        )
      } else {
        Self(self.0.with_raw_http_path(&uri))
      }
    }

    fn method(mut self, method: impl Into<String>) -> Self {
      *self.0.method_mut() = method.into().parse().expect("expected valid method");
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
    async fn get_expected_path(&self) -> String {
      spawn_server(self.get_config()).await
    }

    fn get_config(&self) -> &Config {
      &self.config
    }

    fn get_request(&self) -> LambdaTestRequest<Request> {
      LambdaTestRequest(Request::default())
    }

    async fn test_server(
      &self,
      request: LambdaTestRequest<Request>,
      expected_path: String,
    ) -> TestResponse {
      let router = Router::new(
        Arc::new(self.config.clone().owned_resolvers()),
        self.config.ticket_server().service_info(),
      );

      route_request_to_response(request.0, router, expected_path, &self.config).await
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
    server_tests::test_get::<JsonResponse, _>(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn post_http_tickets() {
    server_tests::test_post::<JsonResponse, _>(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn parameterized_get_http_tickets() {
    server_tests::test_parameterized_get::<JsonResponse, _>(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn parameterized_post_http_tickets() {
    server_tests::test_parameterized_post::<JsonResponse, _>(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn parameterized_post_class_header_http_tickets() {
    server_tests::test_parameterized_post_class_header::<JsonResponse, _>(
      &LambdaTestServer::default(),
    )
    .await;
  }

  #[tokio::test]
  async fn cors_simple_request() {
    cors_tests::test_cors_simple_request(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn cors_preflight_request() {
    cors_tests::test_cors_preflight_request(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn get_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_get::<JsonResponse, _>(&LambdaTestServer::new_with_tls(base_path.path()))
      .await;
  }

  #[tokio::test]
  async fn post_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_post::<JsonResponse, _>(&LambdaTestServer::new_with_tls(base_path.path()))
      .await;
  }

  #[tokio::test]
  async fn parameterized_get_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_parameterized_get::<JsonResponse, _>(&LambdaTestServer::new_with_tls(
      base_path.path(),
    ))
    .await;
  }

  #[tokio::test]
  async fn parameterized_post_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_parameterized_post::<JsonResponse, _>(&LambdaTestServer::new_with_tls(
      base_path.path(),
    ))
    .await;
  }

  #[tokio::test]
  async fn parameterized_post_class_header_https_tickets() {
    let base_path = TempDir::new().unwrap();
    server_tests::test_parameterized_post_class_header::<JsonResponse, _>(
      &LambdaTestServer::new_with_tls(base_path.path()),
    )
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

  #[test]
  fn get_route_invalid_method() {
    let uri = Uri::builder().path_and_query("/reads/id").build().unwrap();
    test_expected_invalid_method(&Method::DELETE, &uri, StatusCode::METHOD_NOT_ALLOWED);
  }

  #[test]
  fn get_route_no_path() {
    let uri = Uri::builder().path_and_query("").build().unwrap();
    test_expected_invalid_method(&Method::GET, &uri, StatusCode::NOT_FOUND);
  }

  #[test]
  fn get_route_no_endpoint() {
    let uri = Uri::builder().path_and_query("/path/").build().unwrap();
    test_expected_invalid_method(&Method::GET, &uri, StatusCode::NOT_FOUND);
  }

  #[test]
  fn get_route_reads_no_id() {
    let uri = Uri::builder().path_and_query("/reads/").build().unwrap();
    test_expected_invalid_method(&Method::GET, &uri, StatusCode::NOT_FOUND);
  }

  #[test]
  fn get_route_variants_no_id() {
    let uri = Uri::builder().path_and_query("/variants/").build().unwrap();
    test_expected_invalid_method(&Method::GET, &uri, StatusCode::NOT_FOUND);
  }

  #[test]
  fn get_route_reads_service_info() {
    let uri = Uri::builder()
      .path_and_query("/reads/service-info")
      .build()
      .unwrap();
    let route = Route::get_route(&Method::GET, &uri).unwrap();
    assert_eq!(
      route,
      Route {
        method: HtsgetMethod::Get,
        endpoint: Endpoint::Reads,
        route_type: RouteType::ServiceInfo
      }
    );
  }

  #[test]
  fn get_route_variants_service_info() {
    let uri = Uri::builder()
      .path_and_query("/variants/service-info")
      .build()
      .unwrap();
    let route = Route::get_route(&Method::GET, &uri).unwrap();
    assert_eq!(
      route,
      Route {
        method: HtsgetMethod::Get,
        endpoint: Endpoint::Variants,
        route_type: RouteType::ServiceInfo
      }
    );
  }

  #[test]
  fn route_get_reads_id() {
    let uri = Uri::builder().path_and_query("/reads/id").build().unwrap();
    let route = Route::get_route(&Method::GET, &uri).unwrap();
    assert_eq!(
      route,
      Route {
        method: HtsgetMethod::Get,
        endpoint: Endpoint::Reads,
        route_type: RouteType::Id("id".to_string())
      }
    );
  }

  #[test]
  fn route_post_reads_id() {
    let uri = Uri::builder().path_and_query("/reads/id").build().unwrap();
    let route = Route::get_route(&Method::POST, &uri).unwrap();
    assert_eq!(
      route,
      Route {
        method: HtsgetMethod::Post,
        endpoint: Endpoint::Reads,
        route_type: RouteType::Id("id".to_string())
      }
    );
  }

  #[test]
  fn route_get_variants_id() {
    let uri = Uri::builder()
      .path_and_query("/variants/id")
      .build()
      .unwrap();
    let route = Route::get_route(&Method::GET, &uri).unwrap();
    assert_eq!(
      route,
      Route {
        method: HtsgetMethod::Get,
        endpoint: Endpoint::Variants,
        route_type: RouteType::Id("id".to_string())
      }
    );
  }

  #[test]
  fn route_post_variants_id() {
    let uri = Uri::builder()
      .path_and_query("/variants/id")
      .build()
      .unwrap();
    let route = Route::get_route(&Method::POST, &uri).unwrap();
    assert_eq!(
      route,
      Route {
        method: HtsgetMethod::Post,
        endpoint: Endpoint::Variants,
        route_type: RouteType::Id("id".to_string())
      }
    );
  }

  fn test_expected_invalid_method(method: &Method, uri: &Uri, expected_status_code: StatusCode) {
    if let Err(err) = Route::get_route(method, uri) {
      let err = err.unwrap();
      assert_eq!(err.status(), expected_status_code);
      assert_eq!(err.body(), &Body::Empty);
    };
  }

  async fn with_router<'a, F, Fut>(test: F, config: &'a Config)
  where
    F: FnOnce(Router<'a, Vec<Resolver>>) -> Fut,
    Fut: Future<Output = ()>,
  {
    let router = Router::new(
      Arc::new(config.clone().owned_resolvers()),
      config.ticket_server().service_info(),
    );
    test(router).await;
  }

  fn get_request_from_file(file_path: &str) -> Request {
    let event = get_test_file(file_path);
    lambda_http::request::from_str(&event).expect("Failed to create lambda request.")
  }

  async fn spawn_server(config: &Config) -> String {
    let mut bind_data_server = BindDataServer::try_from(config.data_server().clone()).unwrap();
    let server = bind_data_server.bind_data_server().await.unwrap();
    let addr = server.local_addr();

    let path = config.data_server().local_path().to_path_buf();
    tokio::spawn(async move { server.serve(path).await.unwrap() });

    expected_url_path(config, addr)
  }

  async fn endpoint_from_file(file_path: &str, class: Class, config: &Config) {
    let expected_path = spawn_server(config).await;

    with_router(
      |router| async move {
        let response = route_request_to_response(
          get_request_from_file(file_path),
          router,
          expected_path,
          config,
        )
        .await;
        test_response::<JsonResponse>(response, class).await;
      },
      config,
    )
    .await;
  }

  async fn test_service_info_from_file(file_path: &str, config: &Config) {
    let expected_path = expected_url_path(config, config.data_server().addr());

    with_router(
      |router| async {
        let response = route_request_to_response(
          get_request_from_file(file_path),
          router,
          expected_path,
          config,
        )
        .await;
        test_response_service_info(&response);
      },
      config,
    )
    .await;
  }

  async fn route_request_to_response<T: HtsGet + Send + Sync + 'static>(
    request: Request,
    router: Router<'_, T>,
    expected_path: String,
    config: &Config,
  ) -> TestResponse {
    let response = ServiceBuilder::new()
      .layer(configure_cors(config.ticket_server().cors().clone()).unwrap())
      .service(service_fn(|event: Request| async {
        router.route_request(event).await
      }))
      .ready()
      .await
      .unwrap()
      .call(request)
      .await
      .expect("failed to route request");

    let status: u16 = response.status().into();
    let body = response.body().to_vec();

    TestResponse::new(status, response.headers().clone(), body, expected_path)
  }
}
