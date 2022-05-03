use std::sync::Arc;

use lambda_http::{service_fn, Error, Request};

use htsget_config::config::Config;
use htsget_config::regex_resolver::RegexResolver;
use htsget_http_lambda::Router;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::storage::axum_server::HttpsFormatter;
use htsget_search::storage::local::LocalStorage;

#[tokio::main]
async fn main() -> Result<(), Error> {
  let config = Config::from_env()?;

  let searcher = Arc::new(HtsGetFromStorage::new(
    LocalStorage::new(
      config.htsget_path,
      config.htsget_resolver,
      HttpsFormatter::from(config.htsget_addr)
    )
    .unwrap(),
  ));

  let router = &Router::new(searcher, &config.htsget_config_service_info);

  let handler = |event: Request| async move { Ok(router.route_request(event).await) };
  lambda_http::run(service_fn(handler)).await?;

  Ok(())
}
