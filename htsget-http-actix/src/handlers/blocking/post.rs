use actix_web::{
  web::{Data, Json, Path},
  Responder,
};

use htsget_http_core::blocking::get_response_for_post_request;
use htsget_http_core::{Endpoint, PostRequest};
use htsget_search::htsget::blocking::HtsGet;

use crate::handlers::handle_response;
use crate::AppState;

/// POST request reads endpoint
pub async fn reads<H: HtsGet>(
  request: Json<PostRequest>,
  Path(id): Path<String>,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  handle_response(get_response_for_post_request(
    &app_state.get_ref().htsget,
    request.into_inner(),
    id,
    Endpoint::Reads,
  ))
}

/// POST request variants endpoint
pub async fn variants<H: HtsGet>(
  request: Json<PostRequest>,
  Path(id): Path<String>,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  handle_response(get_response_for_post_request(
    &app_state.get_ref().htsget,
    request.into_inner(),
    id,
    Endpoint::Variants,
  ))
}
