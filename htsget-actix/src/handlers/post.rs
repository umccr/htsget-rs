use std::collections::HashMap;

use actix_web::web::Query;
use actix_web::{
  web::{Data, Json, Path},
  HttpRequest, Responder,
};
use tracing::info;
use tracing::instrument;

use htsget_http::{post, Endpoint, PostRequest};
use htsget_search::HtsGet;

use crate::handlers::extract_request;
use crate::AppState;

use super::handle_response;

/// POST request reads endpoint
#[instrument(skip(app_state))]
pub async fn reads<H: HtsGet + Clone + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  body: Json<PostRequest>,
  path: Path<String>,
  http_request: HttpRequest,
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
    )
    .await,
  )
}

/// POST request variants endpoint
#[instrument(skip(app_state))]
pub async fn variants<H: HtsGet + Clone + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  body: Json<PostRequest>,
  path: Path<String>,
  http_request: HttpRequest,
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
    )
    .await,
  )
}
