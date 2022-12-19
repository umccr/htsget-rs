use std::sync::Arc;

use htsget_config::config::{DataServerConfig, TicketServerConfig};
use htsget_config::regex_resolver::RegexResolver;
use lambda_http::Error;
use tracing::instrument;

use htsget_http_lambda::{handle_request, Router};
use htsget_http_lambda::{Config, StorageType};
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::storage::data_server::HttpTicketFormatter;
use htsget_search::storage::local::LocalStorage;

#[tokio::main]
async fn main() -> Result<(), Error> {
  Config::setup_tracing()?;
  let config = Config::from_env(Config::parse_args())?;

  let service_info = config.ticket_server().service_info().clone();
  let cors = config.ticket_server().cors().clone();
  let router = &Router::new(Arc::new(config.owned_resolvers()), &service_info);

  handle_request(cors, router).await
}
