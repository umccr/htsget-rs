use std::str::FromStr;
use lambda_http::{Body, IntoResponse, Request, Response, Error};
use lambda_http::http::{Method, Uri};
use htsget_config::config::HtsgetConfig;
use lambda_http::RequestExt;
use regex::Regex;
use htsget_http_core::Endpoint;
use crate::RouteType::{Id, ServiceInfo};

pub async fn lambda_function(request: Request, config: &HtsgetConfig, route_matcher: RouteMatcher) -> Result<impl IntoResponse, Error> {
  // let route = route_matcher.get_route(request.uri());
  // match (request.method(), route) {
  //   (&Method::GET, Some(Route { endpoint: Endpoint::Reads, route_type: ServiceInfo })) => {
  //     unimplemented!()
  //   },
  //   Method::POST => {
  //     unimplemented!()
  //   },
  //   _ => Ok(Response::builder().status(405).body("").unwrap())
  // }
  Ok(Response::builder().status(405).body("").unwrap())
}

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

pub struct RouteMatcher {
  regex: Regex
}

impl RouteMatcher {
  const ENDPOINT_CAPTURE_NAME: &'static str = "endpoint";
  const SERVICE_INFO_CAPTURE_NAME: &'static str = "service_info";
  const ID_CAPTURE_NAME: &'static str = "id";

  pub fn new() -> Self {
    Self { regex: Self::regex_path() }
  }

  pub fn get_route(&self, method: &Method, uri: &Uri) -> Option<Route> {
    let captures = self.regex.captures(uri.path())?;
    let endpoint: Endpoint = Endpoint::from_str(captures.name(Self::ENDPOINT_CAPTURE_NAME)?.as_str()).expect("Expected valid endpoint.");
    let method = match *method {
      Method::GET => Some(HtsgetMethod::Get),
      Method::POST => Some(HtsgetMethod::Post),
      _ => None
    }?;

    if captures.name(Self::SERVICE_INFO_CAPTURE_NAME).is_some() {
      Some(Route::new(method, endpoint, ServiceInfo))
    } else {
      Some(Route::new(method, endpoint, Id(captures.name(Self::ID_CAPTURE_NAME)?.as_str().to_string())))
    }
  }

  /// Regex which matches the relevant parts of a htsget request.
  fn regex_path() -> Regex {
    let pattern= format!(r"^/(?P<{}>reads|variants)/(?:(?P<{}>service-info$)|(?P<{}>.+$))", Self::ENDPOINT_CAPTURE_NAME, Self::SERVICE_INFO_CAPTURE_NAME, Self::ID_CAPTURE_NAME);
    Regex::new(&pattern).expect("Expected valid regex pattern.")
  }
}

#[cfg(test)]
mod tests {
  use lambda_http::http::Uri;
  use lambda_http::Request;
  use htsget_http_core::Endpoint;
  use crate::{Body, HtsgetMethod, Method, Route, RouteMatcher, RouteType};

  #[test]
  fn test_route_matcher_invalid_method() {
    let route_matcher = RouteMatcher::new();
    let uri = Uri::builder().path_and_query("/reads/id").build().unwrap();
    assert!(route_matcher.get_route(&Method::DELETE, &uri).is_none());
  }

  #[test]
  fn test_route_matcher_no_endpoint() {
    let route_matcher = RouteMatcher::new();
    let uri = Uri::builder().path_and_query("/path/").build().unwrap();
    assert!(route_matcher.get_route(&Method::GET, &uri).is_none());
  }

  #[test]
  fn test_route_matcher_reads_no_id() {
    let route_matcher = RouteMatcher::new();
    let uri = Uri::builder().path_and_query("/reads/").build().unwrap();
    assert!(route_matcher.get_route(&Method::GET, &uri).is_none());
  }

  #[test]
  fn test_route_matcher_variants_no_id() {
    let route_matcher = RouteMatcher::new();
    let uri = Uri::builder().path_and_query("/variants/").build().unwrap();
    assert!(route_matcher.get_route(&Method::GET, &uri).is_none());
  }

  #[test]
  fn test_route_matcher_reads_service_info() {
    let route_matcher = RouteMatcher::new();
    let uri = Uri::builder().path_and_query("/reads/service-info").build().unwrap();
    let route = route_matcher.get_route(&Method::GET, &uri);
    assert_eq!(route, Some(Route { method: HtsgetMethod::Get, endpoint: Endpoint::Reads, route_type: RouteType::ServiceInfo }));
  }

  #[test]
  fn test_route_matcher_variants_service_info() {
    let route_matcher = RouteMatcher::new();
    let uri = Uri::builder().path_and_query("/variants/service-info").build().unwrap();
    let route = route_matcher.get_route(&Method::GET, &uri);
    assert_eq!(route, Some(Route { method: HtsgetMethod::Get, endpoint: Endpoint::Variants, route_type: RouteType::ServiceInfo }));
  }

  #[test]
  fn test_route_matcher_reads_id() {
    let route_matcher = RouteMatcher::new();
    let uri = Uri::builder().path_and_query("/reads/id").build().unwrap();
    let route = route_matcher.get_route(&Method::GET, &uri);
    assert_eq!(route, Some(Route { method: HtsgetMethod::Get, endpoint: Endpoint::Reads, route_type: RouteType::Id("id".to_string()) }));
  }

  #[test]
  fn test_route_matcher_variants_id() {
    let route_matcher = RouteMatcher::new();
    let uri = Uri::builder().path_and_query("/variants/id").build().unwrap();
    let route = route_matcher.get_route(&Method::GET, &uri);
    assert_eq!(route, Some(Route { method: HtsgetMethod::Get, endpoint: Endpoint::Variants, route_type: RouteType::Id("id".to_string()) }));
  }
}