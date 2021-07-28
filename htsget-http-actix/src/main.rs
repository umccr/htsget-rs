use actix_web::{web, App, HttpServer};
use htsget_search::{htsget::from_storage::HtsGetFromStorage, storage::local::LocalStorage};
use std::env::args;
mod config;
use config::Config;
mod handlers;
use handlers::{get, post};

const USAGE: &str = r"
This executable doesn't use command line arguments, but there are some environment variables that can be set to configure the HtsGet server:
* HTSGET_IP: The ip to use. Default: 127.0.0.1
* HTSGET_PORT: The port to use. Default: 8080
* HTSGET_PATH: The path to the directory where the server should be started. Default: Actual directory
";

type HtsGetStorage = HtsGetFromStorage<LocalStorage>;
#[actix_web::main]
async fn main() -> std::io::Result<()> {
  if args().len() > 1 {
    // Show help if command line options are provided
    println!("{}", USAGE);
    return Ok(());
  }
  let config = envy::from_env::<Config>().expect("The environment variables weren't properly set!");
  let htsget_path = config.htsget_path;
  HttpServer::new(move || {
    App::new()
      .data(HtsGetFromStorage::new(
        LocalStorage::new(htsget_path.clone())
          .expect("Couldn't create a Storage with the provided path"),
      ))
      .service(
        web::scope("/reads/{id:.+}")
          .route("", web::get().to(get::reads::<HtsGetStorage>))
          .route("", web::post().to(post::reads::<HtsGetStorage>)),
      )
      .service(
        web::scope("/variants/{id:.+}")
          .route("", web::get().to(get::variants::<HtsGetStorage>))
          .route("", web::post().to(post::variants::<HtsGetStorage>)),
      )
  })
  .bind(format!("{}:{}", config.htsget_ip, config.htsget_port))?
  .run()
  .await
}
