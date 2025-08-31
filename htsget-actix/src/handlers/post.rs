use std::collections::HashMap;

use actix_web::web::Query;
use actix_web::{
  HttpRequest, Responder,
  web::{Data, Json, Path},
};
use htsget_http::{Endpoint, PostRequest, post};
use htsget_search::HtsGet;
use tracing::info;
use tracing::instrument;

use super::{extract_request, handle_response};
use crate::AppState;
use crate::middleware::SuppressedRequest;

/// POST request reads endpoint
#[instrument(skip(app_state))]
pub async fn reads<H: HtsGet + Clone + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  http_request: HttpRequest,
  suppressed_request: SuppressedRequest,
  body: Json<PostRequest>,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  let request = extract_request(request, path, http_request);

  info!(body = ?body, "reads endpoint POST request");

  handle_response(
    post(
      app_state.get_ref().htsget.clone(),
      body.into_inner(),
      request,
      Endpoint::Reads,
      suppressed_request.0,
    )
    .await,
  )
}

/// POST request variants endpoint
#[instrument(skip(app_state))]
pub async fn variants<H: HtsGet + Clone + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  http_request: HttpRequest,
  suppressed_request: SuppressedRequest,
  body: Json<PostRequest>,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  let request = extract_request(request, path, http_request);

  info!(body = ?body, "variants endpoint POST request");

  handle_response(
    post(
      app_state.get_ref().htsget.clone(),
      body.into_inner(),
      request,
      Endpoint::Variants,
      suppressed_request.0,
    )
    .await,
  )
}
