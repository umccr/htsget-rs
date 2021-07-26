use actix_web::{web, App, HttpServer};
use htsget_search::{
  htsget::{from_storage::HtsGetFromStorage, HtsGet},
  storage::local::LocalStorage,
};
use std::env::args;
mod config;
use config::Config;
mod handlers;
use handlers::{get, post, reads_service_info, variants_service_info};

const USAGE: &str = r#"
This executable doesn't use command line arguments, but there are some environment variables that can be set to configure the HtsGet server:
* HTSGET_IP: The ip to use. Default: 127.0.0.1
* HTSGET_PORT: The port to use. Default: 8080
* HTSGET_PATH: The path to the directory where the server should be started. Default: Actual directory
The next variables are used to configure the info for the service-info endpoints
* HTSGET_ID: The id of the service. Default: ""
* HTSGET_NAME: The name of the service. Default: "HtsGet service"
* HTSGET_VERSION: The version of the service. Default: ""
* HTSGET_ORGANIZATION_NAME: The name of the organization. Default: "Snake oil"
* HTSGET_ORGANIZATION_URL: The url of the organization. Default: "https://en.wikipedia.org/wiki/Snake_oil"
"#;

type HtsGetStorage = HtsGetFromStorage<LocalStorage>;

pub struct AppState<H: HtsGet> {
  htsget: H,
  config: Config,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  if args().len() > 1 {
    // Show help if command line options are provided
    println!("{}", USAGE);
    return Ok(());
  }
  let config = envy::from_env::<Config>().expect("The environment variables weren't properly set!");
  let address = format!("{}:{}", config.htsget_ip, config.htsget_port);
  let htsget_path = config.htsget_path.clone();
  HttpServer::new(move || {
    App::new()
      .data(AppState {
        htsget: HtsGetFromStorage::new(
          LocalStorage::new(htsget_path.clone())
            .expect("Couldn't create a Storage with the provided path"),
        ),
        config: config.clone(),
      })
      .service(
        web::scope("/reads")
          .route(
            "/service-info",
            web::get().to(reads_service_info::<HtsGetStorage>),
          )
          .route(
            "/service-info",
            web::post().to(reads_service_info::<HtsGetStorage>),
          )
          .route("/{id:.+}", web::get().to(get::reads::<HtsGetStorage>))
          .route("/{id:.+}", web::post().to(post::reads::<HtsGetStorage>)),
      )
      .service(
        web::scope("/variants")
          .route(
            "/service-info",
            web::get().to(variants_service_info::<HtsGetStorage>),
          )
          .route(
            "/service-info",
            web::post().to(variants_service_info::<HtsGetStorage>),
          )
          .route("/{id:.+}", web::get().to(get::variants::<HtsGetStorage>))
          .route("/{id:.+}", web::post().to(post::variants::<HtsGetStorage>)),
      )
  })
  .bind(address)?
  .run()
  .await
}
