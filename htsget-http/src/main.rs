use actix_web::{
  get,
  web::{Path, Query},
  App, HttpServer, Responder,
};
use std::collections::HashMap;

#[get("/reads/{id:.+}")]
async fn reads(
  mut query: Query<HashMap<String, String>>,
  Path(id): Path<String>,
) -> impl Responder {
  query.insert("id".to_string(), id);
  format!("{:?}", query)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  HttpServer::new(|| App::new().service(reads))
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
