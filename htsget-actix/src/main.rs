use tokio::select;
use tracing::debug;

use htsget_actix::run_server;
use htsget_actix::Config;
use htsget_axum::server::data;
use htsget_config::command;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  if let Some(path) = Config::parse_args_with_command(command!())? {
    let config = Config::from_path(&path)?;

    config.setup_tracing()?;

    debug!(config = ?config, "config parsed");

    if config.data_server().enabled() {
      let local_server = data::join_handle(config.data_server().clone()).await?;

      let ticket_server_config = config.ticket_server().clone();
      let service_info = config.service_info().clone();

      select! {
        local_server = local_server => Ok(local_server??),
        actix_server = run_server(
          config.owned_resolvers(),
          ticket_server_config,
          service_info
        )? => actix_server
      }
    } else {
      let ticket_server_config = config.ticket_server().clone();
      let service_info = config.service_info().clone();

      run_server(config.owned_resolvers(), ticket_server_config, service_info)?.await
    }
  } else {
    Ok(())
  }
}
