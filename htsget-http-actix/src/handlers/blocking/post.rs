#[cfg(not(feature = "async"))]
use actix_web::{
  web::{Data, Json, Path},
  Responder,
};

#[cfg(not(feature = "async"))]
use htsget_http_core::blocking::get_response_for_post_request;
#[cfg(not(feature = "async"))]
use htsget_http_core::{Endpoint, PostRequest};
#[cfg(not(feature = "async"))]
use htsget_search::htsget::blocking::HtsGet;

#[cfg(not(feature = "async"))]
use crate::handlers::handle_response;
#[cfg(not(feature = "async"))]
use crate::AppState;

/// POST request reads endpoint
#[cfg(not(feature = "async"))]
pub async fn reads<H: HtsGet>(
  request: Json<PostRequest>,
  path: Path<String>,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  handle_response(get_response_for_post_request(
    &app_state.get_ref().htsget,
    request.into_inner(),
    path.into_inner(),
    Endpoint::Reads,
  ))
}

/// POST request variants endpoint
#[cfg(not(feature = "async"))]
pub async fn variants<H: HtsGet>(
  request: Json<PostRequest>,
  path: Path<String>,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  handle_response(get_response_for_post_request(
    &app_state.get_ref().htsget,
    request.into_inner(),
    path.into_inner(),
    Endpoint::Variants,
  ))
}
