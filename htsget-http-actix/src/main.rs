use actix_web::{
  get,
  http::StatusCode,
  web::{Data, Json, Path, Query},
  App, HttpServer, Responder,
};
use htsget_http_core::get_response;
use htsget_search::{htsget::from_storage::HtsGetFromStorage, storage::local::LocalStorage};
use std::collections::HashMap;

#[get("/reads/{id:.+}")]
async fn reads(
  query: Query<HashMap<String, String>>,
  Path(id): Path<String>,
  shared_state: Data<HtsGetFromStorage<LocalStorage>>,
) -> impl Responder {
  let mut query_information = query.into_inner();
  query_information.insert("id".to_string(), id);
  if let None = query_information.get("format") {
    query_information.insert("format".to_string(), "BAM".to_string());
  }
  handle_request(query_information, shared_state.get_ref())
}

// TODO: Don't accept the variants formats in the reads endpoint and viceversa
#[get("/variants/{id:.+}")]
async fn variants(
  query: Query<HashMap<String, String>>,
  Path(id): Path<String>,
  shared_state: Data<HtsGetFromStorage<LocalStorage>>,
) -> impl Responder {
  let mut query_information = query.into_inner();
  query_information.insert("id".to_string(), id);
  if let None = query_information.get("format") {
    query_information.insert("format".to_string(), "VCF".to_string());
  }
  handle_request(query_information, shared_state.get_ref())
}

fn handle_request(
  query_information: HashMap<String, String>,
  htsget: &HtsGetFromStorage<LocalStorage>,
) -> impl Responder {
  let response = get_response(htsget, &query_information);
  match response {
    Err(error) => {
      let (json, status_code) = error.to_json_representation();
      Json(json).with_status(StatusCode::from_u16(status_code).unwrap())
    }
    Ok(json) => Json(json).with_status(StatusCode::OK),
  }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  HttpServer::new(|| {
    let htsget = HtsGetFromStorage::new(
      LocalStorage::new("data").expect("Couldn't create a Storage with the provided path"),
    );
    App::new().data(htsget).service(reads).service(variants)
  })
  .bind("127.0.0.1:8080")?
  .run()
  .await
}
