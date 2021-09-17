use actix_files::Files;
use std::env::args;
use std::sync::Arc;

//use htsget_http_actix::handlers::blocking::{get, post, reads_service_info, variants_service_info};
use actix_web::{web, App, HttpServer};
use htsget_http_actix::handlers::{
  get as async_get, post as async_post, reads_service_info as async_reads_service_info,
  variants_service_info as async_variants_service_info,
};

use htsget_http_actix::AsyncAppState;
use htsget_http_actix::AsyncHtsGetStorage;
use htsget_id_resolver::RegexResolver;

use htsget_search::storage::blocking::local::LocalStorage;

use htsget_http_actix::config::Config;
use htsget_http_actix::USAGE;

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
  let storage_base_address = format!("{}/data", address);
  let htsget_path = config.htsget_path.clone();
  let regex_match = config.htsget_regex_match.clone();
  let regex_substitution = config.htsget_regex_substitution.clone();
  HttpServer::new(move || {
    App::new()
      .data(AppState {
        htsget: HtsGetFromStorage::new(
          //LocalStorage::new(&htsget_path, &storage_base_address)
          //  .expect("Couldn't create a Storage with the provided path"),
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
      .service(Files::new("/data", htsget_path.clone()))
  })
  .bind(address)?
  .run()
  .await
}
