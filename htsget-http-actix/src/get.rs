use actix_web::{
  get,
  http::StatusCode,
  web::{Data, Json, Path, Query},
  Responder,
};
use htsget_http_core::{get_response_for_get_request, Endpoint};
use htsget_search::{htsget::from_storage::HtsGetFromStorage, storage::local::LocalStorage};
use std::collections::HashMap;

#[get("/reads/{id:.+}")]
pub async fn reads(
  request: Query<HashMap<String, String>>,
  Path(id): Path<String>,
  shared_state: Data<HtsGetFromStorage<LocalStorage>>,
) -> impl Responder {
  let mut query_information = request.into_inner();
  query_information.insert("id".to_string(), id);
  handle_request(query_information, shared_state.get_ref(), Endpoint::Reads)
}

#[get("/variants/{id:.+}")]
pub async fn variants(
  request: Query<HashMap<String, String>>,
  Path(id): Path<String>,
  shared_state: Data<HtsGetFromStorage<LocalStorage>>,
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
  htsget: &HtsGetFromStorage<LocalStorage>,
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
