use std::env::args;

use tokio::select;

use htsget_config::config::{Config, StorageType, USAGE};
use htsget_http_actix::run_server;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::storage::aws::AwsS3Storage;
use htsget_search::storage::axum_server::HttpsFormatter;
use htsget_search::storage::local::LocalStorage;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  if args().len() > 1 {
    // Show help if command line options are provided
    println!("{}", USAGE);
    return Ok(());
  }

  let config = Config::from_env()?;

  match config.htsget_storage_type {
    StorageType::LocalStorage => local_storage_server(config).await,
    #[cfg(feature = "s3-storage")]
    StorageType::AwsS3Storage => s3_storage_server(config).await,
  }
}

async fn local_storage_server(config: Config) -> std::io::Result<()> {
  let formatter = HttpsFormatter::from(config.htsget_addr);
  let mut local_server = formatter.bind_axum_server().await?;

  let searcher = HtsGetFromStorage::<LocalStorage<HttpsFormatter>>::from(
    config.htsget_path.clone(),
    config.htsget_resolver.clone(),
    formatter.clone(),
  )?;
  let local_server = tokio::spawn(async move {
    local_server
      .serve(
        &config.htsget_path,
        &config.htsget_localstorage_key,
        &config.htsget_localstorage_cert,
      )
      .await
  });

  select! {
    local_server = local_server => Ok(local_server??),
    actix_server = run_server(searcher, config.htsget_config_service_info, config.htsget_addr)? => actix_server
  }
}

#[cfg(feature = "s3-storage")]
async fn s3_storage_server(config: Config) -> std::io::Result<()> {
  let searcher =
    HtsGetFromStorage::<AwsS3Storage>::from(config.htsget_s3_bucket, config.htsget_resolver)
      .await?;
  run_server(
    searcher,
    config.htsget_config_service_info,
    config.htsget_addr,
  )?
  .await
}
