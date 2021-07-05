use actix_web::{get, App, HttpRequest, HttpResponse, HttpServer, Responder};

#[get("/reads/{id:.+}")]
async fn reads(request: HttpRequest) -> impl Responder {
  let id = request.match_info().get("id").unwrap().to_string();
  HttpResponse::Ok().body(&id)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  HttpServer::new(|| App::new().service(reads))
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
