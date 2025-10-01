use crate::server::AppState;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::{Extension, Json};
use htsget_http::{Endpoint, PostRequest, post};
use htsget_search::HtsGet;
use http::HeaderMap;
use serde_json::Value;
use std::collections::HashMap;

use super::{extract_request, handle_response};

/// POST request reads endpoint.
pub async fn reads<H: HtsGet + Clone + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  extension: Option<Extension<Value>>,
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
      app_state.package_info.as_ref(),
      extension.map(|extension| extension.0),
    )
    .await,
  )
}

/// POST request variants endpoint.
pub async fn variants<H: HtsGet + Clone + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  extension: Option<Extension<Value>>,
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
      app_state.package_info.as_ref(),
      extension.map(|extension| extension.0),
    )
    .await,
  )
}
