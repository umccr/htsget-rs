use std::env::args;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

use actix_web::{App, HttpServer, rt::System, web};
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::{Client as S3Client, Region};
use futures::join;

use htsget_http_actix::AsyncAppState;
use htsget_http_actix::AsyncHtsGetStorage;
use htsget_http_actix::config::Config;
use htsget_http_actix::handlers::{get, post, reads_service_info, variants_service_info};
use htsget_http_actix::USAGE;
use htsget_id_resolver::RegexResolver;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::storage::aws::AwsS3Storage;

pub type AsyncHtsGetAwsS3Storage = HtsGetFromStorage<AwsS3Storage>;

async fn test_basic_s3() {
  // request the entire file in S3
  let response =
    reqwest::get("http://127.0.0.1:8080/variants/HG00096/HG00096.hard-filtered").await;

  dbg!(response);

  // request a portion of a file in S3
  /* let response = client
    .get("http://127.0.0.1:8080/variants/HG00096/HG00096.hard-filtered.vcf.gz?referenceName=chr1")
    .header("User-Agent", "actix-web/3.0")
    .send()
    .await;

  dbg!(response); */
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  let config = envy::from_env::<Config>().expect("The environment variables weren't properly set!");

  std::env::set_var("RUST_LOG", "debug");
  env_logger::init();

  // load the AWS config once and then put it as a shared object for use throughout
  let aws_config = Arc::new(
    aws_config::from_env()
      .region(
        RegionProviderChain::first_try("ap-southeast-2")
          .or_default_provider()
          .or_else(Region::new("us-east-1")),
      )
      .load()
      .await,
  );

  let srv = HttpServer::new(move || {
    App::new()
      .data(AsyncAppState {
        htsget: Arc::new(AsyncHtsGetAwsS3Storage::new(AwsS3Storage::new(
          S3Client::new(&aws_config),
          String::from("umccr-10g-data-dev"),
        ))),
        config: config.clone(),
      })
      .service(
        web::scope("/variants")
          .route(
            "/{id:.+}",
            web::get().to(get::variants::<AsyncHtsGetAwsS3Storage>),
          )
          .route(
            "/{id:.+}",
            web::post().to(post::variants::<AsyncHtsGetAwsS3Storage>),
          ),
      )
  })
  .bind("127.0.0.1:8080")?
  .workers(1)
  .run();

  test_basic_s3().await;

  // stop server
  srv.stop(true).await;

  Ok(())
}
