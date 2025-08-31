use axum::Extension;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use htsget_config::types;
use htsget_http::{Endpoint, get};
use htsget_search::HtsGet;
use http::HeaderMap;
use std::collections::HashMap;

use crate::server::AppState;

use super::{extract_request, handle_response};

/// GET request reads endpoint.
pub async fn reads<H: HtsGet + Send + Sync + 'static>(
  query: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  suppressed_request: Option<Extension<Option<types::SuppressedRequest>>>,
  State(app_state): State<AppState<H>>,
) -> impl IntoResponse {
  let request = extract_request(query, path, headers);

  handle_response(
    get(
      app_state.htsget,
      request,
      Endpoint::Reads,
      suppressed_request.and_then(|req| req.0),
    )
    .await,
  )
}

/// GET request variants endpoint.
pub async fn variants<H: HtsGet + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  headers: HeaderMap,
  suppressed_request: Option<Extension<Option<types::SuppressedRequest>>>,
  State(app_state): State<AppState<H>>,
) -> impl IntoResponse {
  let request = extract_request(request, path, headers);

  handle_response(
    get(
      app_state.htsget,
      request,
      Endpoint::Variants,
      suppressed_request.and_then(|req| req.0),
    )
    .await,
  )
}
