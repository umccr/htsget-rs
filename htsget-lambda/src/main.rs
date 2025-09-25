use htsget_config::command;
use htsget_config::config::Config;
use htsget_lambda::run_handler;
use lambda_http::Error;
use rustls::crypto::aws_lc_rs;
use std::io;

#[tokio::main]
async fn main() -> Result<(), Error> {
  aws_lc_rs::default_provider()
    .install_default()
    .map_err(|_| io::Error::other("setting crypto provider"))?;

  if let Some(path) = Config::parse_args_with_command(command!())? {
    run_handler(path.as_path()).await
  } else {
    Ok(())
  }
}
