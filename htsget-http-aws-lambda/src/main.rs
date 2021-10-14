extern crate jemallocator;

#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

// use envy;
use lambda_runtime::{ Error };
use lambda_http::{handler};

use htsget_http_aws_lambda::handlers::async_handlers::handle_lambda_request;
// use htsget_http_aws_lambda::config::Config;


#[tokio::main]
async fn main() -> Result<(), Error> {
  // let config = envy::from_env::<Config>().expect("The environment variables weren't properly set!");
  // let address = format!("{}:{}", "aws_api_gw_endpoint", config.htsget_port); // TODO: Fetch api gw URL via aws-sdk-rust calls
  // let htsget_path = config.htsget_path.clone();
  // let regex_match = config.htsget_regex_match.clone();
  // let regex_substitution = config.htsget_regex_substitution.clone();
 
  lambda_runtime::run(handler(handle_lambda_request)).await?;
  Ok(())
}
