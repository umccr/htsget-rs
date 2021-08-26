use std::env::args;
use std::sync::Arc;

use crate::handlers::blocking::{get, post, reads_service_info, variants_service_info};
use actix_web::{web, App, HttpServer};
use config::Config;
use handlers::{
  get as async_get, post as async_post, reads_service_info as async_reads_service_info,
  variants_service_info as async_variants_service_info,
};
use htsget_id_resolver::RegexResolver;
use htsget_search::htsget::blocking::from_storage::HtsGetFromStorage;
use htsget_search::htsget::blocking::HtsGet;
use htsget_search::{
  htsget::{from_storage::HtsGetFromStorage as AsyncHtsGetFromStorage, HtsGet as AsyncHtsGet},
  storage::blocking::local::LocalStorage,
};

mod config;
mod handlers;

const USAGE: &str = r#"
This executable doesn't use command line arguments, but there are some environment variables that can be set to configure the HtsGet server:
* HTSGET_IP: The ip to use. Default: 127.0.0.1
* HTSGET_PORT: The port to use. Default: 8080
* HTSGET_PATH: The path to the directory where the server should be started. Default: Actual directory
* HTSGET_REGEX: The regular expression that should match an ID. Default: ".*"
* HTSGET_REPLACEMENT: The replacement expression. Default: "$0"
For more information about the regex options look in the documentation of the regex crate(https://docs.rs/regex/).
The next variables are used to configure the info for the service-info endpoints
* HTSGET_ID: The id of the service. Default: ""
* HTSGET_NAME: The name of the service. Default: "HtsGet service"
* HTSGET_VERSION: The version of the service. Default: ""
* HTSGET_ORGANIZATION_NAME: The name of the organization. Default: "Snake oil"
* HTSGET_ORGANIZATION_URL: The url of the organization. Default: "https://en.wikipedia.org/wiki/Snake_oil"
* HTSGET_CONTACT_URL: A url to provide contact to the users. Default: "",
* HTSGET_DOCUMENTATION_URL: A link to the documentation. Default: "https://github.com/umccr/htsget-rs/tree/main/htsget-http-actix",
* HTSGET_CREATED_AT: Date of the creation of the service. Default: "",
* HTSGET_UPDATED_AT: Date of the last update of the service. Default: "",
* HTSGET_ENVIRONMENT: The environment in which the service is running. Default: "Testing",
"#;

#[cfg(feature = "async")]
type AsyncHtsGetStorage = AsyncHtsGetFromStorage<LocalStorage>;
type HtsGetStorage = HtsGetFromStorage<LocalStorage>;

#[cfg(feature = "async")]
pub struct AsyncAppState<H: AsyncHtsGet> {
  htsget: Arc<H>,
  config: Config,
}

pub struct AppState<H: HtsGet> {
  htsget: H,
  config: Config,
}

#[cfg(feature = "async")]
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
  let regex_match = config.htsget_regex_match.clone();
  let regex_substitution = config.htsget_regex_substitution.clone();
  HttpServer::new(move || {
    App::new()
      .data(AsyncAppState {
        htsget: Arc::new(AsyncHtsGetStorage::new(
          LocalStorage::new(
            htsget_path.clone(),
            RegexResolver::new(&regex_match, &regex_substitution).unwrap(),
          )
          .expect("Couldn't create a Storage with the provided path"),
        )),
        config: config.clone(),
      })
      .service(
        web::scope("/reads")
          .route(
            "/service-info",
            web::get().to(async_reads_service_info::<AsyncHtsGetStorage>),
          )
          .route(
            "/service-info",
            web::post().to(async_reads_service_info::<AsyncHtsGetStorage>),
          )
          .route(
            "/{id:.+}",
            web::get().to(async_get::reads::<AsyncHtsGetStorage>),
          )
          .route(
            "/{id:.+}",
            web::post().to(async_post::reads::<AsyncHtsGetStorage>),
          ),
      )
      .service(
        web::scope("/variants")
          .route(
            "/service-info",
            web::get().to(async_variants_service_info::<AsyncHtsGetStorage>),
          )
          .route(
            "/service-info",
            web::post().to(async_variants_service_info::<AsyncHtsGetStorage>),
          )
          .route(
            "/{id:.+}",
            web::get().to(async_get::variants::<AsyncHtsGetStorage>),
          )
          .route(
            "/{id:.+}",
            web::post().to(async_post::variants::<AsyncHtsGetStorage>),
          ),
      )
  })
  .bind(address)?
  .run()
  .await
}

#[cfg(not(feature = "async"))]
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
  let regex_match = config.htsget_regex_match.clone();
  let regex_substitution = config.htsget_regex_substitution.clone();
  HttpServer::new(move || {
    App::new()
      .data(AppState {
        htsget: HtsGetFromStorage::new(
          LocalStorage::new(
            htsget_path.clone(),
            RegexResolver::new(&regex_match, &regex_substitution).unwrap(),
          )
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
