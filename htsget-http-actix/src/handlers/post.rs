use actix_web::{
  Responder,
  web::{Data, Json, Path},
};

use htsget_http_core::{Endpoint, get_response_for_post_request, PostRequest};
use htsget_search::htsget::HtsGet;

use crate::AsyncAppState;

use super::handle_response;

/// POST request reads endpoint
pub async fn reads<H: HtsGet + Send + Sync + 'static>(
  request: Json<PostRequest>,
  path: Path<String>,
  app_state: Data<AsyncAppState<H>>,
) -> impl Responder {
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
pub async fn variants<H: HtsGet + Send + Sync + 'static>(
  request: Json<PostRequest>,
  path: Path<String>,
  app_state: Data<AsyncAppState<H>>,
) -> impl Responder {
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
