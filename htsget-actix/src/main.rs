use tokio::select;
use tracing::debug;

use htsget_actix::run_server;
use htsget_actix::Config;
use htsget_axum::data_server::BindDataServer;
use htsget_config::command;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  if let Some(path) =
    Config::parse_args_with_command(command!()).expect("expected valid command parsing")
  {
    let config = Config::from_path(&path)?;

    config.setup_tracing()?;

    debug!(config = ?config, "config parsed");

    if config.data_server().enabled() {
      let server = config.data_server().clone();
      let mut bind_data_server = BindDataServer::from(server.clone());

      let local_server = bind_data_server.bind_data_server().await?;
      let local_server =
        tokio::spawn(async move { local_server.serve(&server.local_path()).await });

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
