use std::env::args;
use std::io::ErrorKind;
use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use futures_util::future::err;
use tokio::select;

use htsget_config::config::{Config, USAGE};
use htsget_http_actix::configure_server;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
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

  let formatter = HttpsFormatter::from(config.htsget_addr);
  let mut local_server = formatter.bind_axum_server().await?;

  let searcher = HtsGetFromStorage::new(
    LocalStorage::new(
      config.htsget_path.clone(),
      config.htsget_resolver,
        formatter
    )?,
  );

  select! {
    local_server = tokio::spawn(async move {
      local_server.serve(&config.htsget_path, &config.htsget_localstorage_key, &config.htsget_localstorage_cert).await
    }) => Ok(local_server??),
    actix_server = HttpServer::new(move || {
      App::new().configure(|service_config: &mut web::ServiceConfig| {
        configure_server(service_config, searcher.clone(), config.htsget_config_service_info.clone());
      })
    })
    .bind(config.htsget_addr)?
    .run() => actix_server
  }
}
