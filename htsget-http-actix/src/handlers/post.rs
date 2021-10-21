use actix_web::{
  web::{Data, Json, Path},
  Responder,
};

use htsget_http_core::{get_response_for_post_request, Endpoint, PostRequest};
use htsget_search::htsget::HtsGet;

use crate::AsyncAppState;

use super::handle_response;

/// POST request reads endpoint
pub async fn reads<H: HtsGet + Send + Sync + 'static>(
  request: Json<PostRequest>,
  path: Path<String>,
  app_state: Data<AsyncAppState<H>>,
) -> impl Responder {
  let (id) = path.into_inner();
  handle_response(
    get_response_for_post_request(
      app_state.get_ref().htsget.clone(),
      request.into_inner(),
      id,
      Endpoint::Reads,
    )
    .await,
  )
}

/// POST request variants endpoint
pub async fn variants<H: HtsGet + Send + Sync + 'static>(
  request: Json<PostRequest>,
  path: Path<String>,
  app_state: Data<AsyncAppState<H>>,
) -> impl Responder {
  let (id) = path.into_inner();
  handle_response(
    get_response_for_post_request(
      app_state.get_ref().htsget.clone(),
      request.into_inner(),
      id,
      Endpoint::Variants,
    )
    .await,
  )
}
