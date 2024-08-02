use std::io;
use tokio::select;
use tracing::debug;

use htsget_axum::server::{data, ticket};
use htsget_config::command;
use htsget_config::config::Config;

#[tokio::main]
async fn main() -> io::Result<()> {
  if let Some(path) =
    Config::parse_args_with_command(command!()).expect("expected valid command parsing")
  {
    let config = Config::from_path(&path)?;

    config.setup_tracing()?;

    debug!(config = ?config, "config parsed");

    if config.data_server().enabled() {
      let local_server = data::join_handle(config.data_server().clone()).await?;
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
