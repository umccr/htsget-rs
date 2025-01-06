use rustls::crypto::aws_lc_rs;
use std::io;
use tokio::select;
use tracing::debug;

use htsget_axum::server::{data, ticket};
use htsget_config::config::data_server::DataServerEnabled;
use htsget_config::config::Config;
use htsget_config::{command, description, package_info, repository};

#[tokio::main]
async fn main() -> io::Result<()> {
  aws_lc_rs::default_provider()
    .install_default()
    .map_err(|_| io::Error::other("setting crypto provider"))?;

  if let Some(path) =
    Config::parse_args_with_command(command!()).expect("expected valid command parsing")
  {
    let mut config = Config::from_path(&path)?;

    config.setup_tracing()?;

    let service_info = config.service_info_mut();
    service_info.insert_package_info(package_info!());
    service_info.insert_repository(repository!());
    service_info.insert_description(description!());

    debug!(config = ?config, "config parsed");

    if let DataServerEnabled::Some(data_server) = config.data_server() {
      let local_server = data::join_handle(data_server.clone()).await?;
      let ticket_server = ticket::join_handle(config).await?;

      select! {
        local_server = local_server => Ok(local_server??),
        axum_server = ticket_server => Ok(axum_server??)
      }
    } else {
      Ok(ticket::join_handle(config).await?.await??)
    }
  } else {
    Ok(())
  }
}
