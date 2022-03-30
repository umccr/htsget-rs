use lambda_http::{Request, service_fn};
use lambda_runtime::Error;
use htsget_config::config::HtsgetConfig;
use htsget_http_lambda::lambda_function;

#[tokio::main]
async fn main() -> Result<(), Error> {
  let config = envy::from_env::<HtsgetConfig>().expect("The environment variables weren't properly set!");
  let config_ref = &config;

  let handler = |event: Request| async move {
    lambda_function(event, config_ref)
  };
  lambda_runtime::run(service_fn(handler)).await?;

  Ok(())
}