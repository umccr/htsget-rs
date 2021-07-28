use super::handle_response;
use actix_web::{
  web::{Data, Path, Query},
  Responder,
};
use htsget_http_core::{get_response_for_get_request, Endpoint};
use htsget_search::htsget::HtsGet;
use std::collections::HashMap;

/// GET request reads endpoint
pub async fn reads<H: HtsGet>(
  request: Query<HashMap<String, String>>,
  Path(id): Path<String>,
  htsget: Data<H>,
) -> impl Responder {
  let mut query_information = request.into_inner();
  query_information.insert("id".to_string(), id);
  handle_response(get_response_for_get_request(
    htsget.get_ref(),
    query_information,
    Endpoint::Reads,
  ))
}

/// GET request variants endpoint
pub async fn variants<H: HtsGet>(
  request: Query<HashMap<String, String>>,
  Path(id): Path<String>,
  htsget: Data<H>,
) -> impl Responder {
  let mut query_information = request.into_inner();
  query_information.insert("id".to_string(), id);
  handle_response(get_response_for_get_request(
    htsget.get_ref(),
    query_information,
    Endpoint::Variants,
  ))
}
