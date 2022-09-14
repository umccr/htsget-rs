use std::env::args;

use std::io::{Error, ErrorKind};

use tokio::select;

use htsget_config::config::{Config, StorageType, USAGE};
use htsget_http_actix::run_server;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::storage::data_server::HttpTicketFormatter;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  Config::setup_tracing()?;

  if args().len() > 1 {
    // Show help if command line options are provided
    println!("{}", USAGE);
    return Ok(());
  }

  let config = Config::from_env()?;

  match config.storage_type {
    StorageType::LocalStorage => local_storage_server(config).await,
    #[cfg(feature = "s3-storage")]
    StorageType::AwsS3Storage => s3_storage_server(config).await,
    _ => Err(Error::new(ErrorKind::Other, "unsupported storage type")),
  }
}

async fn local_storage_server(config: Config) -> std::io::Result<()> {
  let mut formatter = HttpTicketFormatter::try_from(
    config.data_server_addr,
    config.data_server_cert,
    config.data_server_key,
  )?;
  let local_server = formatter.bind_data_server().await?;

  let searcher =
    HtsGetFromStorage::local_from(config.path.clone(), config.resolver.clone(), formatter)?;
  let local_server = tokio::spawn(async move { local_server.serve(&config.path).await });

  select! {
    local_server = local_server => Ok(local_server??),
    actix_server = run_server(searcher, config.service_info, config.ticket_server_addr)? => actix_server
  }
}

#[cfg(feature = "s3-storage")]
async fn s3_storage_server(config: Config) -> std::io::Result<()> {
  let searcher = HtsGetFromStorage::s3_from(config.s3_bucket, config.resolver).await;
  run_server(searcher, config.service_info, config.ticket_server_addr)?.await
}
