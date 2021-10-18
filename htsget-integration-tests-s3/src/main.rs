use std::env::args;

use std::sync::Arc;

use actix_web::{web, App, HttpServer};

use htsget_http_actix::handlers::{get, post, reads_service_info, variants_service_info};
use htsget_http_actix::AsyncAppState;
use htsget_http_actix::AsyncHtsGetStorage;

use htsget_id_resolver::RegexResolver;

use htsget_search::storage::aws::AwsS3Storage;

use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::{Client, Region};
use htsget_http_actix::config::Config;
use htsget_http_actix::USAGE;
use htsget_search::htsget::from_storage::HtsGetFromStorage;

async fn aws_s3_client() -> Client {
  let region_provider = RegionProviderChain::first_try("ap-southeast-2")
    .or_default_provider()
    .or_else(Region::new("us-east-1"));

  let shared_config = aws_config::from_env().region(region_provider).load().await;

  Client::new(&shared_config)
}

type AsyncAwsHtsGet = HtsGetFromStorage<AwsS3Storage>;
struct AsyncAwsAppState {
  pub htsget: AsyncAwsHtsGet,
  pub config: Config,
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
  let htsget_bucket = config.htsget_s3_bucket.as_ref().unwrap().clone();
  let regex_match = config.htsget_regex_match.clone();
  let regex_substitution = config.htsget_regex_substitution.clone();
  let factory = web::Data::new(AsyncAwsAppState {
    htsget: AsyncAwsHtsGet::new(AwsS3Storage::new(aws_s3_client().await, htsget_bucket)),
    config: config.clone(),
  });

  HttpServer::new(move || {
    App::new()
      .data(factory.clone())
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
