use std::env::args;

use std::io::{Error, ErrorKind};

use tokio::select;
use tracing::Dispatch;
use tracing_flame::FlameLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{fmt, EnvFilter, Registry};

use htsget_config::config::{Config, StorageType, USAGE};
use htsget_http_actix::run_server;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::storage::ticket_server::HttpTicketFormatter;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
  let fmt_layer = fmt::Layer::default();
  let (flame_layer, _guard) = FlameLayer::with_file("tracing.folded").unwrap();

  let subscriber = Registry::default()
    .with(env_filter)
    .with(fmt_layer)
    .with(flame_layer);
  let dispatcher = Dispatch::new(subscriber);

  tracing::dispatcher::set_global_default(dispatcher)
    .expect("Failed to install `tracing` dispatch.");

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
    config.ticket_server_addr,
    config.ticket_server_cert,
    config.ticket_server_key,
  )?;
  let local_server = formatter.bind_ticket_server().await?;

  let searcher =
    HtsGetFromStorage::local_from(config.path.clone(), config.resolver.clone(), formatter)?;
  let local_server = tokio::spawn(async move { local_server.serve(&config.path).await });

  select! {
    local_server = local_server => Ok(local_server??),
    actix_server = run_server(searcher, config.service_info, config.addr)? => actix_server
  }
}

#[cfg(feature = "s3-storage")]
async fn s3_storage_server(config: Config) -> std::io::Result<()> {
  let searcher = HtsGetFromStorage::s3_from(config.s3_bucket, config.resolver).await;
  run_server(searcher, config.service_info, config.addr)?.await
}
