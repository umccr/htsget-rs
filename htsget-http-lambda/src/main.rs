use std::sync::Arc;

use lambda_http::Error;
use tracing::instrument;
use htsget_config::config::{LocalDataServer, TicketServerConfig};
use htsget_config::config::aws::AwsS3DataServer;
use htsget_config::regex_resolver::RegexResolver;

use htsget_http_lambda::{handle_request, Router};
use htsget_http_lambda::{Config, StorageType};
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::storage::data_server::HttpTicketFormatter;
use htsget_search::storage::local::LocalStorage;

#[tokio::main]
async fn main() -> Result<(), Error> {
  Config::setup_tracing()?;
  let config = Config::from_env(Config::parse_args())?;

  let resolver = config.resolvers.first().unwrap();
  match resolver.server.clone() {
    StorageType::LocalStorage(server_config) => local_storage_server(&server_config, resolver, config.ticket_server_config).await,
    #[cfg(feature = "s3-storage")]
    StorageType::AwsS3Storage(server_config) => s3_storage_server(&server_config, resolver, config.ticket_server_config).await,
    _ => Err("unsupported storage type".into()),
  }
}

#[instrument(skip_all)]
async fn local_storage_server(config: &LocalDataServer, resolver: &RegexResolver, ticket_config: TicketServerConfig) -> Result<(), Error> {
  let formatter = HttpTicketFormatter::try_from(config.clone())?;
  let searcher: Arc<HtsGetFromStorage<LocalStorage<HttpTicketFormatter>>> = Arc::new(
    HtsGetFromStorage::local_from(config.path.clone(), resolver.clone(), formatter)?,
  );
  let router = &Router::new(searcher, &ticket_config.service_info);

  handle_request(
    config.cors_allow_credentials,
    config.cors_allow_origin.clone(),
    router,
  )
  .await
}

#[cfg(feature = "s3-storage")]
#[instrument(skip_all)]
async fn s3_storage_server(config: &AwsS3DataServer, resolver: &RegexResolver, ticket_config: TicketServerConfig) -> Result<(), Error> {
  let searcher = Arc::new(HtsGetFromStorage::s3_from(config.bucket.clone(), resolver.clone()).await);
  let router = &Router::new(searcher, &ticket_config.service_info);

  handle_request(
    config.cors_allow_credentials,
    config.cors_allow_origin.clone(),
    router,
  )
  .await
}
