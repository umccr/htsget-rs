use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use http::HeaderMap;

use htsget_http::{post, Endpoint, PostRequest};
use htsget_search::HtsGet;

use crate::handlers::extract_request;
use crate::server::AppState;

use super::handle_response;

/// POST request reads endpoint.
pub async fn reads<H: HtsGet + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  State(app_state): State<AppState<H>>,
  Json(body): Json<PostRequest>,
) -> impl IntoResponse {
  let request = extract_request(request, path, headers);

  handle_response(post(app_state.htsget, body, request, Endpoint::Reads).await)
}

/// POST request variants endpoint.
pub async fn variants<H: HtsGet + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  State(app_state): State<AppState<H>>,
  Json(body): Json<PostRequest>,
) -> impl IntoResponse {
  let request = extract_request(request, path, headers);

  handle_response(post(app_state.htsget, body, request, Endpoint::Variants).await)
}
