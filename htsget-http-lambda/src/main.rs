use std::sync::Arc;

use lambda_http::{service_fn, Error, Request};
use tracing::info;
use tracing::instrument;

use htsget_config::config::{Config, StorageType};
use htsget_http_lambda::Router;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::storage::data_server::HttpTicketFormatter;
use htsget_search::storage::local::LocalStorage;

#[tokio::main]
async fn main() -> Result<(), Error> {
  Config::setup_tracing()?;
  let config = Config::from_env()?;

  match config.storage_type {
    StorageType::LocalStorage => local_storage_server(config).await,
    #[cfg(feature = "s3-storage")]
    StorageType::AwsS3Storage => s3_storage_server(config).await,
    _ => Err("unsupported storage type".into()),
  }
}

#[instrument(skip_all)]
async fn local_storage_server(config: Config) -> Result<(), Error> {
  let formatter = HttpTicketFormatter::try_from(config.data_server_config)?;
  let searcher: Arc<HtsGetFromStorage<LocalStorage<HttpTicketFormatter>>> = Arc::new(
    HtsGetFromStorage::local_from(config.path, config.resolver, formatter)?,
  );
  let router = &Router::new(searcher, &config.ticket_server_config.service_info);

  let handler = |event: Request| async move {
    info!(event = ?event, "received request");
    router.route_request(event).await
  };
  lambda_http::run(service_fn(handler)).await?;

  Ok(())
}

#[cfg(feature = "s3-storage")]
#[instrument(skip_all)]
async fn s3_storage_server(config: Config) -> Result<(), Error> {
  let searcher = Arc::new(HtsGetFromStorage::s3_from(config.s3_bucket, config.resolver).await);
  let router = &Router::new(searcher, &config.ticket_server_config.service_info);

  let handler = |event: Request| async move {
    info!(event = ?event, "received request");
    router.route_request(event).await
  };
  lambda_http::run(service_fn(handler)).await?;

  Ok(())
}
