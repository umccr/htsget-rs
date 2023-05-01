use std::sync::Arc;

use lambda_http::Error;

use htsget_lambda::Config;
use htsget_lambda::{handle_request, Router};

#[tokio::main]
async fn main() -> Result<(), Error> {
  Config::setup_tracing()?;

  if let Some(path) = Config::parse_args() {
    let config = Config::from_path(&path)?;

    let service_info = config.service_info().clone();
    let cors = config.ticket_server().cors().clone();
    let router = &Router::new(Arc::new(config.owned_resolvers()), &service_info);

    handle_request(cors, router).await
  } else {
    Ok(())
  }
}
