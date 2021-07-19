use actix_web::{
  http::StatusCode,
  post,
  web::{Data, Json, Path},
  Responder,
};
use htsget_http_core::{get_response_for_post_request, PostRequest};
use htsget_search::{htsget::from_storage::HtsGetFromStorage, storage::local::LocalStorage};

// TODO: Don't accept the variants formats in the reads endpoint and viceversa
#[post("/reads/{id:.+}")]
pub async fn reads(
  request: Json<PostRequest>,
  Path(id): Path<String>,
  shared_state: Data<HtsGetFromStorage<LocalStorage>>,
) -> impl Responder {
  let mut request = request.into_inner();
  if request.format.is_none() {
    request.format = Some("BAM".to_string());
  }
  handle_request(request, id, shared_state.get_ref())
}

#[post("/variants/{id:.+}")]
pub async fn variants(
  request: Json<PostRequest>,
  Path(id): Path<String>,
  shared_state: Data<HtsGetFromStorage<LocalStorage>>,
) -> impl Responder {
  let mut request = request.into_inner();
  if request.format.is_none() {
    request.format = Some("VCF".to_string());
  }
  handle_request(request, id, shared_state.get_ref())
}

fn handle_request(
  request: PostRequest,
  id: String,
  htsget: &HtsGetFromStorage<LocalStorage>,
) -> impl Responder {
  let response = get_response_for_post_request(htsget, request, id);
  match response {
    Err(error) => {
      let (json, status_code) = error.to_json_representation();
      Json(json).with_status(StatusCode::from_u16(status_code).unwrap())
    }
    Ok(json) => Json(json).with_status(StatusCode::OK),
  }
}
