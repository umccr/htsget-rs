use std::sync::Arc;

use lambda_http::{service_fn, Error, Request};
use tracing::info;

use htsget_config::config::{Config, StorageType};
use htsget_http_lambda::Router;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::storage::axum_server::HttpsFormatter;

#[tokio::main]
async fn main() -> Result<(), Error> {
  tracing_subscriber::fmt::init();
  let config = Config::from_env()?;

  match config.storage_type {
    StorageType::LocalStorage => local_storage_server(config).await,
    #[cfg(feature = "s3-storage")]
    StorageType::AwsS3Storage => s3_storage_server(config).await,
  }
}

async fn local_storage_server(config: Config) -> Result<(), Error> {
  let searcher = Arc::new(HtsGetFromStorage::local_from(
    config.path,
    config.resolver,
    HttpsFormatter::from(config.addr),
  )?);
  let router = &Router::new(searcher, &config.service_info);

  let handler = |event: Request| async move {
    info!(event = ?event, "Received request");
    Ok(router.route_request(event).await?)
  };
  lambda_http::run(service_fn(handler)).await?;

  Ok(())
}

#[cfg(feature = "s3-storage")]
async fn s3_storage_server(config: Config) -> Result<(), Error> {
  let searcher = Arc::new(HtsGetFromStorage::s3_from(config.s3_bucket, config.resolver).await);
  let router = &Router::new(searcher, &config.service_info);

  let handler = |event: Request| async move {
    info!(event = ?event, "Received request");
    Ok(router.route_request(event).await?)
  };
  lambda_http::run(service_fn(handler)).await?;

  Ok(())
}
