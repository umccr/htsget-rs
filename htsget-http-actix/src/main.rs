use actix_web::{App, HttpServer};
use htsget_search::{htsget::from_storage::HtsGetFromStorage, storage::local::LocalStorage};
mod get;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  HttpServer::new(|| {
    let htsget = HtsGetFromStorage::new(
      LocalStorage::new("data").expect("Couldn't create a Storage with the provided path"),
    );
    App::new()
      .data(htsget)
      .service(get::reads)
      .service(get::variants)
  })
  .bind("127.0.0.1:8080")?
  .run()
  .await
}
