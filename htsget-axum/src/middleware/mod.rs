//! Middleware components for the htsget-rs server.
//!

use crate::error::{HtsGetError, HtsGetResult};
use axum::RequestExt;
use axum::extract::{Query, Request};
use htsget_config::types;
use http::HeaderMap;
use std::collections::HashMap;

pub mod auth;

pub(crate) async fn extract_request(request: &mut Request) -> HtsGetResult<types::Request> {
  let query = request
    .extract_parts::<Query<HashMap<String, String>>>()
    .await
    .map_err(|err| HtsGetError::permission_denied(err.to_string()))?;
  let headers = request
    .extract_parts::<HeaderMap>()
    .await
    .map_err(|err| HtsGetError::permission_denied(err.to_string()))?;

  Ok(types::Request::new(
    request.uri().path().to_string(),
    query.0,
    headers,
  ))
}
