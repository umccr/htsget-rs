use lambda_http::{Body, IntoResponse, Request, Response};
use lambda_runtime::Error;
use htsget_config::config::HtsgetConfig;

pub async fn lambda_function(request: Request, config: &HtsgetConfig) -> Result<impl IntoResponse, Error> {
  Ok(Response::builder().status(400).body("").unwrap())
}