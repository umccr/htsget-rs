use actix_web::{
  get,
  web::{Path, Query},
  App, HttpServer, Responder,
};
use htsget_http::response::get_response;
use std::collections::HashMap;

#[get("/reads/{id:.+}")]
async fn reads(query: Query<HashMap<String, String>>, Path(id): Path<String>) -> impl Responder {
  let mut query_information = query.into_inner();
  query_information.insert("id".to_string(), id);
  let response = get_response(query_information);
  response.unwrap_err().to_json_responder()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  HttpServer::new(|| App::new().service(reads))
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
