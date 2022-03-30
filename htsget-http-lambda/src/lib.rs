use lambda_http::{IntoResponse, Request};
use lambda_http::http::Error;
use htsget_config::config::HtsgetConfig;

pub async fn lambda_function(request: Request, config: &HtsgetConfig) -> Result<impl IntoResponse, Error> {
  unimplemented!();
}