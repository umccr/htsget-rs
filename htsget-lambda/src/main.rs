use std::sync::Arc;

use lambda_http::Error;
use tracing::debug;

use htsget_config::command;
use htsget_lambda::Config;
use htsget_lambda::{handle_request, Router};

#[tokio::main]
async fn main() -> Result<(), Error> {
  if let Some(path) =
    Config::parse_args_with_command(command!()).expect("expected valid command parsing")
  {
    let config = Config::from_path(&path)?;

    config.setup_tracing()?;

    debug!(config = ?config, "config parsed");

    let service_info = config.service_info().clone();
    let cors = config.ticket_server().cors().clone();
    let router = &Router::new(Arc::new(config.owned_resolvers()), &service_info);

    handle_request(cors, router).await
  } else {
    Ok(())
  }
}
