#[cfg(not(feature = "async"))]
use std::collections::HashMap;

#[cfg(not(feature = "async"))]
use actix_web::{
  web::{Data, Path, Query},
  Responder,
};

#[cfg(not(feature = "async"))]
use htsget_http_core::blocking::get_response_for_get_request;
#[cfg(not(feature = "async"))]
use htsget_http_core::Endpoint;
#[cfg(not(feature = "async"))]
use htsget_search::htsget::blocking::HtsGet;

#[cfg(not(feature = "async"))]
use crate::handlers::handle_response;
#[cfg(not(feature = "async"))]
use crate::AppState;

/// GET request reads endpoint
#[cfg(not(feature = "async"))]
pub fn reads<H: HtsGet>(
  request: Query<HashMap<String, String>>,
  Path(id): Path<String>,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  let mut query_information = request.into_inner();
  query_information.insert("id".to_string(), id);
  handle_response(get_response_for_get_request(
    &app_state.get_ref().htsget,
    query_information,
    Endpoint::Reads,
  ))
}

/// GET request variants endpoint
#[cfg(not(feature = "async"))]
pub fn variants<H: HtsGet>(
  request: Query<HashMap<String, String>>,
  Path(id): Path<String>,
  app_state: Data<AppState<H>>,
) -> impl Responder {
  let mut query_information = request.into_inner();
  query_information.insert("id".to_string(), id);
  handle_response(get_response_for_get_request(
    &app_state.get_ref().htsget,
    query_information,
    Endpoint::Variants,
  ))
}
