use std::collections::HashMap;

use actix_web::web::{Path, Query};
use actix_web::{Either, HttpRequest, Responder, http::StatusCode};
use htsget_config::types::{JsonResponse, Request};
use htsget_http::Result;
use http::{HeaderMap as HttpHeaderMap, HeaderName, Method};
use pretty_json::PrettyJson;

pub use crate::handlers::service_info::{
  get_service_info_json, reads_service_info, variants_service_info,
};

pub mod get;
pub mod post;
pub mod service_info;

pub(crate) mod pretty_json;

pub(crate) struct HeaderMap(HttpHeaderMap);

impl HeaderMap {
  pub(crate) fn into_inner(self) -> HttpHeaderMap {
    self.0
  }
}

impl From<&HttpRequest> for HeaderMap {
  fn from(http_request: &HttpRequest) -> Self {
    HeaderMap(HttpHeaderMap::from_iter(http_request.headers().clone()))
  }
}

/// Handles a response, converting errors to json and using the proper HTTP status code
pub(crate) fn handle_response(
  response: Result<JsonResponse>,
) -> Either<impl Responder, impl Responder> {
  match response {
    Err(error) => {
      let (json, status_code) = error.to_json_representation();
      Either::Left(
        PrettyJson(json)
          .customize()
          .with_status(HttpVersionCompat::status_code_1_to_0_2(status_code)),
      )
    }
    Ok(json) => Either::Right(PrettyJson(json).customize().with_status(StatusCode::OK)),
  }
}

pub(crate) fn extract_request_path(
  request: Query<HashMap<String, String>>,
  path: String,
  http_request: HttpRequest,
) -> Request {
  let query = request.into_inner();

  Request::new(
    path,
    query,
    HttpVersionCompat::header_map_0_2_to_1(HeaderMap::from(&http_request).into_inner()),
  )
}

pub(crate) fn extract_request(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  http_request: HttpRequest,
) -> Request {
  extract_request_path(request, path.into_inner(), http_request)
}

// Todo, remove this when actix-web starts using http 1.0.
pub(crate) struct HttpVersionCompat;

impl HttpVersionCompat {
  pub(crate) fn header_names_1_to_0_2(header_name: Vec<http_1::HeaderName>) -> Vec<HeaderName> {
    header_name
      .iter()
      .map(|name| name.as_str().parse().ok())
      .collect::<Option<_>>()
      .unwrap_or_default()
  }

  pub(crate) fn methods_0_2_to_1(method: Vec<http_1::Method>) -> Vec<Method> {
    method
      .iter()
      .map(|method| method.as_str().parse().ok())
      .collect::<Option<_>>()
      .unwrap_or_default()
  }

  pub(crate) fn header_map_0_2_to_1(header_map: HttpHeaderMap) -> http_1::HeaderMap {
    // Silently ignore incompatible headers. This isn't ideal but it shouldn't cause any errors.
    header_map
      .iter()
      .map(|(name, value)| {
        let name = name.as_str().parse().ok()?;
        let value = value.to_str().ok()?.parse().ok()?;

        Some((name, value))
      })
      .collect::<Option<Vec<_>>>()
      .map(FromIterator::from_iter)
      .unwrap_or_default()
  }

  pub(crate) fn status_code_1_to_0_2(status_code: http_1::StatusCode) -> StatusCode {
    // Report an error if the status code is not convertible
    StatusCode::from_u16(status_code.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
  }
}
