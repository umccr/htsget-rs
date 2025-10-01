use std::collections::HashMap;

use actix_web::web::{Query, ReqData};
use actix_web::{
  HttpRequest, Responder,
  web::{Data, Json, Path},
};
use htsget_http::{Endpoint, PostRequest, post};
use htsget_search::HtsGet;
use serde_json::Value;
use tracing::info;
use tracing::instrument;

use super::{extract_request, handle_response};
use crate::AppState;

/// POST request reads endpoint
#[instrument(skip(app_state))]
pub async fn reads<H: HtsGet + Clone + Send + Sync + 'static>(
  request: Query<HashMap<String, String>>,
  path: Path<String>,
  http_request: HttpRequest,
  extension: Option<ReqData<Value>>,
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
      app_state.auth.clone(),
      app_state.package_info.as_ref(),
      extension.map(|extension| extension.into_inner()),
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
  extension: Option<ReqData<Value>>,
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
      app_state.auth.clone(),
      app_state.package_info.as_ref(),
      extension.map(|extension| extension.into_inner()),
    )
    .await,
  )
}
