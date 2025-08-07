use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use http::HeaderMap;

use htsget_http::{Endpoint, get};
use htsget_search::HtsGet;

use crate::handlers::extract_request;
use crate::server::AppState;

use super::handle_response;

/// GET request reads endpoint.
pub async fn reads<H: HtsGet + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  State(app_state): State<AppState<H>>,
) -> impl IntoResponse {
  let request = extract_request(request, path, headers);

  handle_response(get(app_state.htsget, request, Endpoint::Reads).await)
}

/// GET request variants endpoint.
pub async fn variants<H: HtsGet + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  State(app_state): State<AppState<H>>,
) -> impl IntoResponse {
  let request = extract_request(request, path, headers);

  handle_response(get(app_state.htsget, request, Endpoint::Variants).await)
}
