//! Library providing the routing and http responses for Cloudflare worker requests.
//!

use worker::*;
mod utils;

use std::collections::HashMap;
use std::sync::Arc;

use http::Uri;

use htsget_config::config::cors::CorsConfig;
pub use htsget_config::config::{Config, DataServerConfig, ServiceInfo, TicketServerConfig};
pub use htsget_config::regex_resolver::StorageType;
use htsget_http::{Endpoint, PostRequest};
use htsget_search::htsget::HtsGet;
use htsget_search::storage::configure_cors;

use crate::handlers::get::get;
use crate::handlers::post::post;
use crate::handlers::service_info::get_service_info_json;

pub mod handlers;


fn log_request(req: &Request) {
  console_log!(
    "{} - [{}], located at: {:?}, within: {}",
    Date::now().to_string(),
    req.path(),
    req.cf().coordinates().unwrap_or_default(),
    req.cf().region().unwrap_or_else(|| "unknown region".into())
  );
}

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
  pub async fn route_request(&self, request: Request) -> http::Result<Response<ResponseBody>> {
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
            .status(http::StatusCode::UNSUPPORTED_MEDIA_TYPE)
            .body(ResponseBody::Empty)?,
        ),
        Some(query) => post(id, self.searcher.clone(), query, endpoint).await,
      },
      _ => Ok(
        Response::builder()
          .status(http::StatusCode::METHOD_NOT_ALLOWED)
          .body(ResponseBody::Empty)?,
      ),
    }
  }

  /// Extracts post request query parameters.
  fn extract_query_from_payload(request: &Request) -> Option<PostRequest> {
    if request.body().is_empty() {
      Some(PostRequest::default())
    } else {
      let payload = request.payload::<PostRequest>();
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
    query
  }
}

pub async fn handle_request<H>(cors: CorsConfig, router: &Router<'_, H>) -> Result<(), Error>
where
  H: HtsGet + Send + Sync + 'static,
{
  // Optionally, get more helpful error messages written to the console in the case of a panic.
  utils::set_panic_hook();

  let router = worker::Router::new();

  // let handler =
  //   ServiceBuilder::new()
  //     .layer(cors_layer)
  //     .service(service_fn(|event: Request| async move {
  //       info!(event = ?event, "received request");
  //       router.route_request(event).await
  //     }));

  // lambda_http::run(handler).await?;

  Ok(())
}