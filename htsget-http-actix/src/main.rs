use actix_web::{web::Data, App, HttpServer};
use htsget_search::{htsget::from_storage::HtsGetFromStorage, storage::local::LocalStorage};
use std::env::args;
mod config;
use config::Config;
mod get;
mod post;

const USAGE: &str = r"
There are some environment variables that can be set to configure the server:
* HTSGET_IP: The ip to use. Default: 127.0.0.1
* HTSGET_PORT: The port to use. Default: 8080
* HTSGET_PATH: The path to the directory where the server should be started. Default: Actual directory
";

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  if args().any(|arg| arg == "-h" || arg == "--help") {
    println!("{}", USAGE);
    return Ok(());
  }
  let config = envy::from_env::<Config>().expect("The environment variables weren't properly set!");
  let htsget = HtsGetFromStorage::new(
    LocalStorage::new(config.htsget_path)
      .expect("Couldn't create a Storage with the provided path"),
  );
  let htsget = Data::new(htsget);
  HttpServer::new(move || {
    App::new()
      .app_data(htsget.clone())
      .service(get::reads)
      .service(get::variants)
      .service(post::reads)
      .service(post::variants)
  })
  .bind(format!("{}:{}", config.htsget_ip, config.htsget_port))?
  .run()
  .await
}
