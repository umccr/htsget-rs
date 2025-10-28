use super::{extract_request, handle_response};
use crate::server::AppState;
use axum::Extension;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use htsget_http::{Endpoint, get};
use htsget_search::HtsGet;
use http::HeaderMap;
use serde_json::Value;
use std::collections::HashMap;
use tracing::debug;

/// GET request reads endpoint.
pub async fn reads<H: HtsGet + Send + Sync + 'static>(
  query: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  extension: Option<Extension<Value>>,
  State(app_state): State<AppState<H>>,
) -> impl IntoResponse {
  let request = extract_request(query, path, headers);

  handle_response(
    get(
      app_state.htsget,
      request,
      Endpoint::Reads,
      app_state.auth_middleware,
      app_state.package_info.as_ref(),
      extension.map(|extension| extension.0),
    )
    .await,
  )
}

/// GET request variants endpoint.
pub async fn variants<H: HtsGet + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  extension: Option<Extension<Value>>,
  State(app_state): State<AppState<H>>,
) -> impl IntoResponse {
  debug!("query {:?}", request);
  debug!("path {:?}", path);
  debug!("headers {:?}", headers);

  let request = extract_request(request, path, headers);

  handle_response(
    get(
      app_state.htsget,
      request,
      Endpoint::Variants,
      app_state.auth_middleware,
      app_state.package_info.as_ref(),
      extension.map(|extension| extension.0),
    )
    .await,
  )
}
