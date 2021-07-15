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
  let response = get_response(shared_state.get_ref(), &query_information);
  let (json, status_code) = response.unwrap_err().to_json_representation();
  Json(json).with_status(StatusCode::from_u16(status_code).unwrap())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  HttpServer::new(|| {
    App::new()
      .data(HtsGetFromStorage::new(LocalStorage::new("data")))
      .service(reads)
  })
  .bind("127.0.0.1:8080")?
  .run()
  .await
}
