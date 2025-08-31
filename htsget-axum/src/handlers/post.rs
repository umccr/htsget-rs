use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::{Extension, Json};
use htsget_config::types;
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
  suppressed_request: Option<Extension<Option<types::SuppressedRequest>>>,
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
      suppressed_request.and_then(|req| req.0),
    )
    .await,
  )
}

/// POST request variants endpoint.
pub async fn variants<H: HtsGet + Clone + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  suppressed_request: Option<Extension<Option<types::SuppressedRequest>>>,
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
      suppressed_request.and_then(|req| req.0),
    )
    .await,
  )
}
