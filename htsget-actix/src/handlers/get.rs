use std::collections::HashMap;

use actix_web::{
  web::{Data, Path, Query},
  HttpRequest, Responder,
};
use tracing::info;
use tracing::instrument;

use htsget_http::{get, Endpoint};
use htsget_search::HtsGet;

use crate::handlers::extract_request;
use crate::AppState;

use super::handle_response;

/// GET request reads endpoint
#[instrument(skip(app_state))]
pub async fn reads<H: HtsGet + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  http_request: HttpRequest,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  let request = extract_request(request, path, http_request);

  info!(request = ?request, "reads endpoint GET request");

  handle_response(get(app_state.get_ref().htsget.clone(), request, Endpoint::Reads).await)
}

/// GET request variants endpoint
#[instrument(skip(app_state))]
pub async fn variants<H: HtsGet + Send + Sync + 'static>(
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
    )
    .await,
  )
}
