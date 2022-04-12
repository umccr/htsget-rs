pub mod handlers;

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use lambda_http::{Body, IntoResponse, Request, Response, Error};
use lambda_http::http::{Method, StatusCode, Uri};
use htsget_config::config::HtsgetConfig;
use regex::Regex;
use lambda_http::ext::RequestExt;
use lambda_http::http::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;
use htsget_http_core::{Endpoint, PostRequest};
use htsget_search::htsget::HtsGet;
use crate::handlers::service_info::get_service_info_json;
use crate::handlers::get::get;
use crate::handlers::post::post;

#[derive(Debug, PartialEq)]
pub struct Route {
  method: HtsgetMethod,
  endpoint: Endpoint,
  route_type: RouteType
}

#[derive(Debug, PartialEq)]
pub enum HtsgetMethod {
  Get,
  Post
}

#[derive(Debug, PartialEq)]
pub enum RouteType {
  ServiceInfo,
  Id(String)
}

impl Route {
  pub fn new(method: HtsgetMethod, endpoint: Endpoint, route_type: RouteType) -> Self {
    Self { method, endpoint, route_type }
  }
}

pub struct Router<'a, H> {
  searcher: Arc<H>,
  config: &'a HtsgetConfig
}

impl<'a, H: HtsGet + Send + Sync + 'static> Router<'a, H> {
  pub fn new(searcher: Arc<H>, config: &'a HtsgetConfig) -> Self {
    Self { searcher, config }
  }

  pub fn get_route(&self, method: &Method, uri: &Uri) -> Option<Route> {
    let with_endpoint = |endpoint: Endpoint, endpoint_type: &str| {
      if endpoint_type.is_empty() {
        let method = match *method {
          Method::GET => Some(HtsgetMethod::Get),
          Method::POST => Some(HtsgetMethod::Post),
          _ => None
        }?;
        if endpoint_type == "service-info" {
          Some(Route::new(method, endpoint, RouteType::ServiceInfo))
        } else {
          Some(Route::new(method, endpoint, RouteType::Id(endpoint_type.to_string())))
        }
      } else {
        None
      }
    };

    if let Some(reads) = uri.path().strip_prefix("/reads/") {
      with_endpoint(Endpoint::Reads, reads)
    } else if let Some(variants) = uri.path().strip_prefix("/variants/") {
      with_endpoint(Endpoint::Variants, variants)
    } else {
      None
    }
  }

  pub async fn route_request(&self, request: Request) -> Response<Body> {
    match self.get_route(request.method(), request.uri()) {
      Some(Route { method: _, endpoint, route_type: RouteType::ServiceInfo }) => {
        get_service_info_json(self.searcher.clone(), endpoint, self.config).into_response()
      },
      Some(Route { method: HtsgetMethod::Get, endpoint, route_type: RouteType::Id(id) }) => {
        get(id, self.searcher.clone(), Self::extract_query(&request), endpoint).await.into_response()
      },
      Some(Route { method: HtsgetMethod::Post, endpoint, route_type: RouteType::Id(id) }) => {
        match Self::extract_query_from_payload(&request) {
          None => Response::builder().status(StatusCode::UNSUPPORTED_MEDIA_TYPE).body("").unwrap().into_response(),
          Some(query) => post(id, self.searcher.clone(), query, endpoint).await.into_response()
        }
      },
      _ => Response::builder().status(StatusCode::METHOD_NOT_ALLOWED).body("").unwrap().into_response()
    }
  }

  fn extract_query_from_payload(request: &Request) -> Option<PostRequest> {
    // Check if the content type is application/json
    let content_type = request.headers().get(CONTENT_TYPE)?;
    if content_type.to_str().ok()? != mime::APPLICATION_JSON.as_ref() {
      return None;
    }

    request.payload().ok()?
  }

  /// Extract a query hashmap from a request.
  fn extract_query(request: &Request) -> HashMap<String, String> {
    let mut query = HashMap::new();
    // Silently ignores all but the last query key, for keys that are present more than once.
    // This is the way actix-web does it, but should we return an error instead if a key is present
    // more than once?
    for (key, value) in request.query_string_parameters().iter() {
      query.insert(key.to_string(), value.to_string());
    }
    query
  }

}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;
  use std::future::Future;
  use std::path::{Path, PathBuf};
  use std::str::FromStr;
  use std::sync::Arc;
  use lambda_http::{http, Request, RequestExt};
  use lambda_http::http::{HeaderValue, Uri};
  use serde::Serialize;
  use htsget_config::config::HtsgetConfig;
  use htsget_config::regex_resolver::RegexResolver;
  use htsget_http_core::{Endpoint, JsonResponse};
  use htsget_search::htsget::{Format, Headers, Url};
  use htsget_search::htsget::from_storage::HtsGetFromStorage;
  use htsget_search::storage::local::LocalStorage;
  use htsget_test_utils::{TestRequest, Header, TestServer, Response, server_tests};
  use crate::{Body, HtsgetMethod, Method, Route, Router, RouteType, StatusCode};
  use async_trait::async_trait;
  use lambda_http::Body::Text;
  use lambda_http::http::header::HeaderName;
  use query_map::QueryMap;

  struct LambdaTestServer {
    config: HtsgetConfig
  }

  struct LambdaTestRequest<T>(T);

  impl TestRequest for LambdaTestRequest<Request> {
    fn insert_header(mut self, header: Header<impl Into<String>>) -> Self {
      self.0.headers_mut().insert(HeaderName::from_str(&header.name.into()).expect("Expected valid header name."), header.value.into().parse().expect("Expected valid header value."));
      self
    }

    fn set_payload(mut self, payload: impl Into<String>) -> Self {
      *self.0.body_mut() = Text(payload.into());
      self
    }

    fn uri(mut self, uri: impl Into<String>) -> Self {
      *self.0.uri_mut() = uri.into().parse().expect("Expected valid uri.");
      if let Some(query) = self.0.uri().query().map(|s| s.to_string()) {
        Self(self.0.with_query_string_parameters(query.parse::<QueryMap>().expect("Expected valid query parameters.")))
      } else {
        self
      }
    }

    fn method(mut self, method: impl Into<String>) -> Self {
      *self.0.method_mut() = method.into().parse().expect("Expected valid method.");
      self
    }
  }

  impl Default for LambdaTestServer {
    fn default() -> Self {
      std::env::set_var(
        "HTSGET_PATH",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
      );
      let config =
        envy::from_env::<HtsgetConfig>().expect("The environment variables weren't properly set!");
      Self { config }
    }
  }

  #[async_trait(?Send)]
  impl TestServer<LambdaTestRequest<Request>> for LambdaTestServer {
    fn get_config(&self) -> &HtsgetConfig {
      &self.config
    }

    fn get_request(&self) -> LambdaTestRequest<Request> {
      LambdaTestRequest(Request::default())
    }

    async fn test_server(&self, request: LambdaTestRequest<Request>) -> Response {
      let router = Router::new(Arc::new(HtsGetFromStorage::new(
        LocalStorage::new(
          &self.config.htsget_path,
          RegexResolver::new(&self.config.htsget_regex_match, &self.config.htsget_regex_substitution).unwrap(),
        ).expect("Couldn't create a Storage with the provided path"))), &self.config);

      let response = router.route_request(request.0).await;
      let status: u16 = response.status().into();
      let body = response.body().to_vec().into();
      Response::new(status, body)
    }
  }

  #[tokio::test]
  async fn test_get() {
    server_tests::test_get(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn test_post() {
    server_tests::test_post(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn test_parameterized_get() {
    server_tests::test_parameterized_get(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn test_parameterized_post() {
    server_tests::test_parameterized_post(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn test_service_info() {
    server_tests::test_service_info(&LambdaTestServer::default()).await;
  }

  #[tokio::test]
  async fn get_route_invalid_method() {
    let config = HtsgetConfig::default();
    with_router(|router| async move {
      let uri = Uri::builder().path_and_query("/reads/id").build().unwrap();
      assert!(router.get_route(&Method::DELETE, &uri).is_none());
    }, &config).await;
  }

  #[tokio::test]
  async fn get_route_no_path() {
    let config = HtsgetConfig::default();
    with_router(|router| async move {
      let uri = Uri::builder().path_and_query("").build().unwrap();
      assert!(router.get_route(&Method::GET, &uri).is_none());
    }, &config).await;
  }

  #[tokio::test]
  async fn get_route_no_endpoint() {
    let config = HtsgetConfig::default();
    with_router(|router| async move {
      let uri = Uri::builder().path_and_query("/path/").build().unwrap();
      assert!(router.get_route(&Method::GET, &uri).is_none());
    }, &config).await;
  }

  #[tokio::test]
  async fn get_route_reads_no_id() {
    let config = HtsgetConfig::default();
    with_router(|router| async move {
      let uri = Uri::builder().path_and_query("/reads/").build().unwrap();
      assert!(router.get_route(&Method::GET, &uri).is_none());
    }, &config).await;
  }

  #[tokio::test]
  async fn get_route_variants_no_id() {
    let config = HtsgetConfig::default();
    with_router(|router| async move {
      let uri = Uri::builder().path_and_query("/variants/").build().unwrap();
      assert!(router.get_route(&Method::GET, &uri).is_none());
    }, &config).await;
  }

  #[tokio::test]
  async fn get_route_reads_service_info() {
    let config = HtsgetConfig::default();
    with_router(|router| async move {
      let uri = Uri::builder().path_and_query("/reads/service-info").build().unwrap();
      let route = router.get_route(&Method::GET, &uri);
      assert_eq!(route, Some(Route { method: HtsgetMethod::Get, endpoint: Endpoint::Reads, route_type: RouteType::ServiceInfo }));
    }, &config).await;
  }

  #[tokio::test]
  async fn get_route_variants_service_info() {
    let config = HtsgetConfig::default();
    with_router(|router| async move {
      let uri = Uri::builder().path_and_query("/variants/service-info").build().unwrap();
      let route = router.get_route(&Method::GET, &uri);
      assert_eq!(route, Some(Route { method: HtsgetMethod::Get, endpoint: Endpoint::Variants, route_type: RouteType::ServiceInfo }));
    }, &config).await;
  }

  #[tokio::test]
  async fn get_route_reads_id() {
    let config = HtsgetConfig::default();
    with_router(|router| async move {
      let uri = Uri::builder().path_and_query("/reads/id").build().unwrap();
      let route = router.get_route(&Method::GET, &uri);
      assert_eq!(route, Some(Route { method: HtsgetMethod::Get, endpoint: Endpoint::Reads, route_type: RouteType::Id("id".to_string()) }));
    }, &config).await;
  }

  #[tokio::test]
  async fn get_route_variants_id() {
    let config = HtsgetConfig::default();
    with_router(|router| async move {
      let uri = Uri::builder().path_and_query("/variants/id").build().unwrap();
      let route = router.get_route(&Method::GET, &uri);
      assert_eq!(route, Some(Route { method: HtsgetMethod::Get, endpoint: Endpoint::Variants, route_type: RouteType::Id("id".to_string()) }));
    }, &config).await;
  }

  async fn with_router<'a, F, Fut>(test: F, config: &'a HtsgetConfig)
    where
      F: FnOnce(Router<'a, HtsGetFromStorage<LocalStorage>>) -> Fut,
      Fut: Future<Output = ()>
  {
    let router = Router::new(Arc::new(HtsGetFromStorage::new(
      LocalStorage::new(&config.htsget_path, RegexResolver::new(&config.htsget_regex_match, &config.htsget_regex_substitution).unwrap()).unwrap()
    )), config);
    test(router).await
  }

}