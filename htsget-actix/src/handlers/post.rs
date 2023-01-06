use actix_web::{
  web::{Data, Json, Path},
  Responder,
};
use tracing::info;
use tracing::instrument;

use htsget_http::{get_response_for_post_request, Endpoint, PostRequest};
use htsget_search::htsget::HtsGet;

use crate::AppState;

use super::handle_response;

/// POST request reads endpoint
#[instrument(skip(app_state))]
pub async fn reads<H: HtsGet + Send + Sync + 'static>(
  request: Json<PostRequest>,
  path: Path<String>,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  info!(request = ?request, "reads endpoint POST request");
  handle_response(
    get_response_for_post_request(
      app_state.get_ref().htsget.clone(),
      request.into_inner(),
      path.into_inner(),
      Endpoint::Reads,
    )
    .await,
  )
}

/// POST request variants endpoint
#[instrument(skip(app_state))]
pub async fn variants<H: HtsGet + Send + Sync + 'static>(
  request: Json<PostRequest>,
  path: Path<String>,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  info!(request = ?request, "variants endpoint POST request");
  handle_response(
    get_response_for_post_request(
      app_state.get_ref().htsget.clone(),
      request.into_inner(),
      path.into_inner(),
      Endpoint::Variants,
    )
    .await,
  )
}
