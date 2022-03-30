use std::env::args;

use actix_web::{web, App, HttpServer};

#[cfg(feature = "async")]
use htsget_http_actix::async_configure_server as configure_server;
#[cfg(not(feature = "async"))]
use htsget_http_actix::configure_server;

use htsget_config::config::{HtsgetConfig, USAGE};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  if args().len() > 1 {
    // Show help if command line options are provided
    println!("{}", USAGE);
    return Ok(());
  }

  let config =
    envy::from_env::<HtsgetConfig>().expect("The environment variables weren't properly set!");
  let address = format!("{}:{}", config.htsget_ip, config.htsget_port);
  HttpServer::new(move || {
    App::new().configure(|service_config: &mut web::ServiceConfig| {
      configure_server(service_config, config.clone());
    })
  })
  .bind(address)?
  .run()
  .await
}
