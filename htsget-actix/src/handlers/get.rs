use std::collections::HashMap;

use actix_web::{
  HttpRequest, Responder,
  web::{Data, Path, Query},
};
use htsget_http::{Endpoint, get};
use htsget_search::HtsGet;
use tracing::info;
use tracing::instrument;

use super::handle_response;
use crate::AppState;
use crate::handlers::extract_request;

/// GET request reads endpoint
#[instrument(skip(app_state))]
pub async fn reads<H: HtsGet + Clone + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  http_request: HttpRequest,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  let request = extract_request(request, path, http_request);

  info!(request = ?request, "reads endpoint GET request");

  handle_response(
    get(
      app_state.get_ref().htsget.clone(),
      request,
      Endpoint::Reads,
      app_state.auth.clone(),
    )
    .await,
  )
}

/// GET request variants endpoint
#[instrument(skip(app_state))]
pub async fn variants<H: HtsGet + Clone + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  http_request: HttpRequest,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  let request = extract_request(request, path, http_request);

  info!(request = ?request, "variants endpoint GET request");

  handle_response(
    get(
      app_state.get_ref().htsget.clone(),
      request,
      Endpoint::Variants,
      app_state.auth.clone(),
    )
    .await,
  )
}
