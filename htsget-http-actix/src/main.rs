use std::env::args;

#[cfg(feature = "async")]
use std::sync::Arc;

use actix_web::{web, App, HttpServer};

// Async
#[cfg(feature = "async")]
use htsget_http_actix::handlers::{get, post, reads_service_info, variants_service_info};
#[cfg(feature = "async")]
use htsget_http_actix::AsyncAppState;
#[cfg(feature = "async")]
use htsget_http_actix::AsyncHtsGetStorage;

// Blocking
#[cfg(not(feature = "async"))]
use htsget_http_actix::handlers::blocking::{get, post, reads_service_info, variants_service_info};
#[cfg(not(feature = "async"))]
use htsget_http_actix::AppState;
#[cfg(not(feature = "async"))]
use htsget_http_actix::HtsGetStorage;
#[cfg(not(feature = "async"))]
use htsget_search::htsget::blocking::from_storage::HtsGetFromStorage;

use htsget_id_resolver::RegexResolver;

use htsget_search::storage::blocking::local::LocalStorage;

use htsget_http_actix::config::Config;
use htsget_http_actix::USAGE;

#[cfg(feature = "async")]
#[actix_web::main]
async fn main() -> std::io::Result<()> {
  color_backtrace::install();

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
      .app_data(web::Data::new(AsyncAppState {
        htsget: Arc::new(AsyncHtsGetStorage::new(
          LocalStorage::new(
            htsget_path.clone(),
            RegexResolver::new(&regex_match, &regex_substitution).unwrap(),
          )
          .expect("Couldn't create a Storage with the provided path"),
        )),
        config: config.clone(),
      }))
      .service(
        web::scope("/reads")
          .route(
            "/service-info",
            web::get().to(reads_service_info::<AsyncHtsGetStorage>),
          )
          .route(
            "/service-info",
            web::post().to(reads_service_info::<AsyncHtsGetStorage>),
          )
          .route("/{id:.+}", web::get().to(get::reads::<AsyncHtsGetStorage>))
          .route(
            "/{id:.+}",
            web::post().to(post::reads::<AsyncHtsGetStorage>),
          ),
      )
      .service(
        web::scope("/variants")
          .route(
            "/service-info",
            web::get().to(variants_service_info::<AsyncHtsGetStorage>),
          )
          .route(
            "/service-info",
            web::post().to(variants_service_info::<AsyncHtsGetStorage>),
          )
          .route(
            "/{id:.+}",
            web::get().to(get::variants::<AsyncHtsGetStorage>),
          )
          .route(
            "/{id:.+}",
            web::post().to(post::variants::<AsyncHtsGetStorage>),
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
      .app_data(web::Data::new(AppState {
        htsget: HtsGetFromStorage::new(
          LocalStorage::new(
            htsget_path.clone(),
            RegexResolver::new(&regex_match, &regex_substitution).unwrap(),
          )
          .expect("Couldn't create a Storage with the provided path"),
        ),
        config: config.clone(),
      }))
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
