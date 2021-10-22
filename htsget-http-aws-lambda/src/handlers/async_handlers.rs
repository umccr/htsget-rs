use std::sync::Arc;
use super::Config;

use lambda_runtime::{ Context, Error };
use lambda_http::{ Request, Response, IntoResponse };

use htsget_search::htsget::HtsGet;
use htsget_http_core::Endpoint;
use htsget_http_core::get_service_info_json as get_base_service_info_json;
use htsget_id_resolver::RegexResolver;

use crate::handlers::PrettyJson;

use crate::handlers::fill_out_service_info_json;
use crate::AsyncAppState;
use crate::AsyncHtsGetStorage;

/// Gets the JSON to return for the reads service-info endpoint
pub async fn reads_service_info<H: HtsGet + Send + Sync + 'static>(
  app_state: AsyncAppState<H>,
) -> Response<H> {
  get_service_info_json(&app_state, Endpoint::Reads)
}

/// Gets the JSON to return for a service-info endpoint
pub fn get_service_info_json<H: HtsGet + Send + Sync + 'static>(
  app_state: &AsyncAppState<H>,
  endpoint: Endpoint,
) -> Response<H> {
  PrettyJson(fill_out_service_info_json(
    get_base_service_info_json(endpoint, app_state.htsget.clone()),
    &app_state.config,
  ))
}

pub async fn lambda_request(req: Request, _: Context) -> Result<impl IntoResponse, Error> {
    // TODO: Route logic here for the different endpoints
    // /reads/{service-info}
    // /variants/{service-info}
    // Handle routes here perhaps? Using "path parameters" in lambda_http:
    // https://github.com/awslabs/aws-lambda-rust-runtime/blob/master/lambda-http/src/ext.rs#L10

    // Path and methods received by the lambda
    let path = req.uri().path();
    //let method = *req.method();
    
    // HtsGet config
    let config = envy::from_env::<Config>().expect("The environment variables weren't properly set!");
    let address = format!("{}:{}", "todo_api_gatewayv2_endpoint", config.htsget_port);
    let htsget_path = config.htsget_path.clone();
    let regex_match = config.htsget_regex_match.clone();
    let regex_substitution = config.htsget_regex_substitution.clone();

    let app_data = AsyncAppState {
      htsget: Arc::new(AsyncHtsGetStorage::new(
        S3Storage::new(
          path,
          RegexResolver::new(&regex_match, &regex_substitution).unwrap(),
        )
        .expect("Couldn't create a Storage with the provided path"),
      )),
      config: config.clone(),
    };
  
    match Some(path) {
      Some("/reads") => {
          Ok(Response::builder()
            .status(200)
            .body(reads_service_info(app_data).await))
          },
      Some("/variants") => unimplemented!(),
      // _ => Ok(Ok(Response::builder()
      // .status(400)
      // .body(Body::from("Error".to_string()))
      // .expect("htsget error"))
      // .ok()
    }
}