use tokio::select;
use tracing::debug;

use htsget_axum::server::BindServer;
use htsget_config::command;
use htsget_config::config::Config;

#[tokio::main]
async fn main() -> std::io::Result<()> {
  if let Some(path) =
    Config::parse_args_with_command(command!()).expect("expected valid command parsing")
  {
    let config = Config::from_path(&path)?;

    config.setup_tracing()?;

    debug!(config = ?config, "config parsed");

    if config.data_server().enabled() {
      let data_server = config.data_server().clone();
      let local_server = BindServer::from(data_server.clone())
        .bind_data_server(data_server.serve_at().to_string())
        .await?;
      let local_server =
        tokio::spawn(async move { local_server.serve(&data_server.local_path()).await });

      let ticket_server_config = config.ticket_server().clone();
      let service_info = config.service_info().clone();
      let ticket_server = BindServer::from(ticket_server_config)
        .bind_ticket_server(config.owned_resolvers(), service_info)
        .await?;
      let ticket_server = tokio::spawn(async move { ticket_server.serve().await });

      select! {
        local_server = local_server => Ok(local_server??),
        axum_server = ticket_server => Ok(axum_server??)
      }
    } else {
      let ticket_server_config = config.ticket_server().clone();
      let service_info = config.service_info().clone();

      Ok(
        BindServer::from(ticket_server_config)
          .bind_ticket_server(config.owned_resolvers(), service_info)
          .await?
          .serve()
          .await?,
      )
    }
  } else {
    Ok(())
  }
}
