use actix_web::{
  http::StatusCode,
  web::{Data, Json, Path, Query},
  Responder,
};
use htsget_http_core::{get_response_for_get_request, Endpoint};
use htsget_search::htsget::HtsGet;
use std::collections::HashMap;

/// GET request reads endpoint
pub async fn reads<H: HtsGet>(
  request: Query<HashMap<String, String>>,
  Path(id): Path<String>,
  shared_state: Data<H>,
) -> impl Responder {
  let mut query_information = request.into_inner();
  query_information.insert("id".to_string(), id);
  handle_request(query_information, shared_state.get_ref(), Endpoint::Reads)
}

/// GET request variants endpoint
pub async fn variants<H: HtsGet>(
  request: Query<HashMap<String, String>>,
  Path(id): Path<String>,
  shared_state: Data<H>,
) -> impl Responder {
  let mut query_information = request.into_inner();
  query_information.insert("id".to_string(), id);
  handle_request(
    query_information,
    shared_state.get_ref(),
    Endpoint::Variants,
  )
}

fn handle_request(
  request_information: HashMap<String, String>,
  htsget: &impl HtsGet,
  endpoint: Endpoint,
) -> impl Responder {
  let response = get_response_for_get_request(htsget, request_information, endpoint);
  match response {
    Err(error) => {
      let (json, status_code) = error.to_json_representation();
      Json(json).with_status(StatusCode::from_u16(status_code).unwrap())
    }
    Ok(json) => Json(json).with_status(StatusCode::OK),
  }
}
