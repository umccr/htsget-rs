use std::io::{Error, ErrorKind};

use tokio::select;
use htsget_config::config::aws::AwsS3DataServer;
use htsget_config::config::{LocalDataServer, TicketServerConfig};
use htsget_config::regex_resolver::RegexResolver;

use htsget_http_actix::run_server;
use htsget_http_actix::{Config, StorageType};
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::storage::data_server::HttpTicketFormatter;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  Config::setup_tracing()?;
  let config = Config::from_env(Config::parse_args())?;

  let resolver = config.resolvers.first().unwrap();
  match resolver.server.clone() {
    StorageType::LocalStorage(server_config) => local_storage_server(server_config.clone(), resolver, config.ticket_server_config).await,
    #[cfg(feature = "s3-storage")]
    StorageType::AwsS3Storage(server_config) => s3_storage_server(&server_config, resolver, config.ticket_server_config).await,
    _ => Err(Error::new(ErrorKind::Other, "unsupported storage type")),
  }
}

async fn local_storage_server(config: LocalDataServer, resolver: &RegexResolver, ticket_config: TicketServerConfig) -> std::io::Result<()> {
  let mut formatter = HttpTicketFormatter::try_from(config.clone())?;
  let local_server = formatter.bind_data_server().await?;

  let searcher =
    HtsGetFromStorage::local_from(config.path.clone(), resolver.clone(), formatter)?;
  let local_server = tokio::spawn(async move { local_server.serve(&config.path.clone()).await });

  select! {
    local_server = local_server => Ok(local_server??),
    actix_server = run_server(
      searcher,
      ticket_config,
    )? => actix_server
  }
}

#[cfg(feature = "s3-storage")]
async fn s3_storage_server(config: &AwsS3DataServer, resolver: &RegexResolver, ticket_config: TicketServerConfig) -> std::io::Result<()> {
  let searcher = HtsGetFromStorage::s3_from(config.bucket.clone(), resolver.clone()).await;
  run_server(searcher, ticket_config)?.await
}
