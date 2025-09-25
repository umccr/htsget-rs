use htsget_config::command;
use htsget_config::config::Config;
use htsget_lambda::run_handler;
use lambda_http::Error;
use rustls::crypto::aws_lc_rs;
use std::env::var;
use std::io;

#[tokio::main]
async fn main() -> Result<(), Error> {
  aws_lc_rs::default_provider()
    .install_default()
    .map_err(|_| io::Error::other("setting crypto provider"))?;

  // Ignore the API gateway stage. This value must be set for the Lambda to function.
  // See https://github.com/awslabs/aws-lambda-rust-runtime/tree/main/lambda-http#integration-with-api-gateway-stages
  let _ = var("AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH")?;

  if let Some(path) = Config::parse_args_with_command(command!())? {
    run_handler(path.as_path()).await
  } else {
    Ok(())
  }
}
