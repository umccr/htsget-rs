use rustls::crypto::aws_lc_rs;
use std::io;
use tokio::select;
use tracing::debug;

use htsget_actix::Config;
use htsget_actix::run_server;
use htsget_axum::server::data;
use htsget_config::config::data_server::DataServerEnabled;
use htsget_config::{command, package_info};

#[actix_web::main]
async fn main() -> io::Result<()> {
  aws_lc_rs::default_provider()
    .install_default()
    .map_err(|_| io::Error::other("setting crypto provider"))?;

  if let Some(path) = Config::parse_args_with_command(command!())? {
    let mut config = Config::from_path(&path)?;

    config.setup_tracing()?;

    let service_info = config.service_info_mut();
    service_info.set_from_package_info(package_info!())?;

    debug!(config = ?config, "config parsed");

    if let DataServerEnabled::Some(data_server) = config.data_server() {
      let local_server = data::join_handle(data_server.clone()).await?;

      let ticket_server_config = config.ticket_server().clone();
      let service_info = config.service_info().clone();

      select! {
        local_server = local_server => Ok(local_server??),
        actix_server = run_server(
          config.into_locations(),
          ticket_server_config,
          service_info
        )? => actix_server
      }
    } else {
      let ticket_server_config = config.ticket_server().clone();
      let service_info = config.service_info().clone();

      run_server(config.into_locations(), ticket_server_config, service_info)?.await
    }
  } else {
    Ok(())
  }
}
