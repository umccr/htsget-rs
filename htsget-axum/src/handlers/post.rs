use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use htsget_http::{Endpoint, PostRequest, post};
use htsget_search::HtsGet;
use http::HeaderMap;
use std::collections::HashMap;

use crate::server::AppState;

use super::{extract_request, handle_response};

/// POST request reads endpoint.
pub async fn reads<H: HtsGet + Clone + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  State(app_state): State<AppState<H>>,
  Json(body): Json<PostRequest>,
) -> impl IntoResponse {
  let request = extract_request(request, path, headers);

  handle_response(
    post(
      app_state.htsget,
      body,
      request,
      Endpoint::Reads,
      app_state.auth_middleware,
    )
    .await,
  )
}

/// POST request variants endpoint.
pub async fn variants<H: HtsGet + Clone + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  State(app_state): State<AppState<H>>,
  Json(body): Json<PostRequest>,
) -> impl IntoResponse {
  let request = extract_request(request, path, headers);

  handle_response(
    post(
      app_state.htsget,
      body,
      request,
      Endpoint::Variants,
      app_state.auth_middleware,
    )
    .await,
  )
}
