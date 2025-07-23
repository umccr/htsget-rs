use std::collections::HashMap;

use axum::extract::{Path, Query};
use axum::response::IntoResponse;
use axum_extra::response::ErasedJson;
use http::{HeaderMap, StatusCode};

use crate::error::HtsGetError;
pub use crate::handlers::service_info::{
  get_service_info_json, reads_service_info, variants_service_info,
};
use htsget_config::types::{JsonResponse, Request};

pub mod get;
pub mod post;
pub mod service_info;

/// Handles a response, converting errors to json and using the proper HTTP status code
fn handle_response(response: htsget_http::Result<JsonResponse>) -> impl IntoResponse {
  match response {
    Err(error) => HtsGetError(error).into_response(),
    Ok(json) => (StatusCode::OK, ErasedJson::pretty(json)).into_response(),
  }
}

fn extract_request(
  Query(query): Query<HashMap<String, String>>,
  Path(path): Path<String>,
  headers: HeaderMap,
) -> Request {
  Request::new(path, query, headers)
}
