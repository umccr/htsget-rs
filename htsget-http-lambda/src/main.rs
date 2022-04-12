use std::sync::Arc;

use lambda_http::{service_fn, Error, Request};

use htsget_config::config::HtsgetConfig;
use htsget_config::regex_resolver::RegexResolver;
use htsget_http_lambda::Router;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::storage::local::LocalStorage;

#[tokio::main]
async fn main() -> Result<(), Error> {
  let config =
    &envy::from_env::<HtsgetConfig>().expect("The environment variables weren't properly set!");

  let htsget_path = config.htsget_path.clone();
  let searcher = Arc::new(HtsGetFromStorage::new(
    LocalStorage::new(
      htsget_path,
      RegexResolver::new(
        &config.htsget_regex_match,
        &config.htsget_regex_substitution,
      )
      .unwrap(),
    )
    .unwrap(),
  ));

  let router = &Router::new(searcher, config);

  let handler = |event: Request| async move { Ok(router.route_request(event).await) };
  lambda_http::run(service_fn(handler)).await?;

  Ok(())
}
