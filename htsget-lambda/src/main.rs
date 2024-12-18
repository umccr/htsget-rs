use htsget_axum::server::ticket::TicketServer;
use htsget_config::command;
use htsget_config::config::Config;
use lambda_http::{run, Error};
use rustls::crypto::aws_lc_rs;
use std::env::set_var;
use std::io;
use tracing::debug;

#[tokio::main]
async fn main() -> Result<(), Error> {
  aws_lc_rs::default_provider()
    .install_default()
    .map_err(|_| io::Error::other("setting crypto provider"))?;

  // Ignore the API gateway stage.
  // See https://github.com/awslabs/aws-lambda-rust-runtime/tree/main/lambda-http#integration-with-api-gateway-stages
  set_var("AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH", "true");

  if let Some(path) = Config::parse_args_with_command(command!())? {
    let config = Config::from_path(&path)?;

    config.setup_tracing()?;

    debug!(config = ?config, "config parsed");

    let service_info = config.service_info().clone();
    let cors = config.ticket_server().cors().clone();
    let router = TicketServer::router(config.into_locations(), service_info, cors);

    run(router).await
  } else {
    Ok(())
  }
}
