use lambda_http::Error;
use std::sync::Arc;

use htsget_http_lambda::Config;
use htsget_http_lambda::{handle_request, Router};

#[tokio::main]
async fn main() -> Result<(), Error> {
  Config::setup_tracing()?;

  if let Some(config) = Config::parse_args() {
    let config = Config::from_env(config)?;

    let service_info = config.ticket_server().service_info().clone();
    let cors = config.ticket_server().cors().clone();
    let router = &Router::new(Arc::new(config.owned_resolvers()), &service_info);

    handle_request(cors, router).await
  } else {
    Ok(())
  }
}
