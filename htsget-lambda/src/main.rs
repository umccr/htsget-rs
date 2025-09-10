use htsget_axum::server::ticket::TicketServer;
use htsget_config::config::Config;
use htsget_config::{command, package_info};
use lambda_http::{run, http::{StatusCode, Response}, service_fn, Error, IntoResponse, Request, RequestPayloadExt, RequestExt};
use rustls::crypto::aws_lc_rs;
use std::io;
use tracing::debug;

#[tokio::main]
async fn main() -> Result<(), Error> {
  aws_lc_rs::default_provider()
    .install_default()
    .map_err(|_| io::Error::other("setting crypto provider"))?;

  // Ignore the API gateway stage. This value must be set for the Lambda to function.
  // See https://github.com/awslabs/aws-lambda-rust-runtime/tree/main/lambda-http#integration-with-api-gateway-stages
  // let _ = var("AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH")?;

  if let Some(path) = Config::parse_args_with_command(command!())? {
    let mut config = Config::from_path(&path)?;

    config.setup_tracing()?;

    let service_info = config.service_info_mut();
    service_info.set_from_package_info(package_info!())?;

    debug!(config = ?config, "config parsed");

    let service_info = config.service_info().clone();
    let cors = config.ticket_server().cors().clone();
    let auth = config.ticket_server().auth().cloned();
    let router = TicketServer::router(config.into_locations(), service_info, cors, auth)?;

    // run(router).await
    run(service_fn(function_handler)).await
  } else {
    Ok(())
  }
}

pub async fn function_handler(event: Request) -> Result<impl IntoResponse, Error> {
  // let body = event.payload::<MyPayload>()?;

  debug!("Received a request: {:?}", event);
  debug!("Request context: {:?}", event.request_context());

  let response = Response::builder()
      .status(StatusCode::OK)
      .header("Content-Type", "text/plain")
      .body("hello".to_string())
      .map_err(Box::new)?;

  Ok(response)
}