use std::collections::HashMap;

use axum::extract::{Path, Query};
use axum::response::IntoResponse;
use axum_extra::response::ErasedJson;
use http::{HeaderMap, StatusCode};

use htsget_config::types::{JsonResponse, Request};

pub use crate::handlers::service_info::{
  get_service_info_json, reads_service_info, variants_service_info,
};

pub mod get;
pub mod post;
pub mod service_info;

/// Handles a response, converting errors to json and using the proper HTTP status code
fn handle_response(response: htsget_http::Result<JsonResponse>) -> (StatusCode, impl IntoResponse) {
  match response {
    Err(error) => {
      let (json, status_code) = error.to_json_representation();
      (status_code, ErasedJson::pretty(json))
    }
    Ok(json) => (StatusCode::OK, ErasedJson::pretty(json)),
  }
}

fn extract_request(
  Query(query): Query<HashMap<String, String>>,
  Path(path): Path<String>,
  headers: HeaderMap,
) -> Request {
  Request::new(path, query, headers)
}
