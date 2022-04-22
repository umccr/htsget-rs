use std::sync::Arc;

use lambda_http::{service_fn, Error, Request};

use htsget_config::config::Config;
use htsget_config::regex_resolver::RegexResolver;
use htsget_http_lambda::Router;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::storage::local::LocalStorage;
use htsget_search::storage::local_server::LocalStorageServer;

#[tokio::main]
async fn main() -> Result<(), Error> {
  let config =
    &envy::from_env::<Config>().expect("The environment variables weren't properly set!");

  let htsget_path = config.htsget_path.clone();
  let searcher = Arc::new(HtsGetFromStorage::new(
    LocalStorage::new(
      htsget_path,
      RegexResolver::new(
        &config.htsget_regex_match,
        &config.htsget_regex_substitution,
      )
      .unwrap(),
      LocalStorageServer::new(&config.htsget_localstorage_ip, &config.htsget_localstorage_port)
    )
    .unwrap(),
  ));

  let router = &Router::new(searcher, config);

  let handler = |event: Request| async move { Ok(router.route_request(event).await) };
  lambda_http::run(service_fn(handler)).await?;

  Ok(())
}
