use actix_web::{
  http::StatusCode,
  post,
  web::{Data, Json, Path},
  Responder,
};
use htsget_http_core::{get_response_for_post_request, Endpoint, PostRequest};
use htsget_search::{htsget::from_storage::HtsGetFromStorage, storage::local::LocalStorage};

/// POST request reads endpoint
#[post("/reads/{id:.+}")]
pub async fn reads(
  request: Json<PostRequest>,
  Path(id): Path<String>,
  shared_state: Data<HtsGetFromStorage<LocalStorage>>,
) -> impl Responder {
  handle_request(
    request.into_inner(),
    id,
    shared_state.get_ref(),
    Endpoint::Reads,
  )
}

/// POST request variants endpoint
#[post("/variants/{id:.+}")]
pub async fn variants(
  request: Json<PostRequest>,
  Path(id): Path<String>,
  shared_state: Data<HtsGetFromStorage<LocalStorage>>,
) -> impl Responder {
  handle_request(
    request.into_inner(),
    id,
    shared_state.get_ref(),
    Endpoint::Variants,
  )
}

fn handle_request(
  request: PostRequest,
  id: String,
  htsget: &HtsGetFromStorage<LocalStorage>,
  endpoint: Endpoint,
) -> impl Responder {
  let response = get_response_for_post_request(htsget, request, id, endpoint);
  match response {
    Err(error) => {
      let (json, status_code) = error.to_json_representation();
      Json(json).with_status(StatusCode::from_u16(status_code).unwrap())
    }
    Ok(json) => Json(json).with_status(StatusCode::OK),
  }
}
