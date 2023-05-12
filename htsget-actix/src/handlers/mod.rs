use actix_web::{http::StatusCode, Either, Responder};

use htsget_config::types::JsonResponse;
use htsget_http::Result;
use pretty_json::PrettyJson;

pub use crate::handlers::service_info::{
  get_service_info_json, reads_service_info, variants_service_info,
};

pub mod get;
pub mod post;
pub mod service_info;

mod pretty_json;

/// Handles a response, converting errors to json and using the proper HTTP status code
fn handle_response(response: Result<JsonResponse>) -> Either<impl Responder, impl Responder> {
  match response {
    Err(error) => {
      let (json, status_code) = error.to_json_representation();
      Either::Left(PrettyJson(json).customize().with_status(status_code))
    }
    Ok(json) => Either::Right(PrettyJson(json).customize().with_status(StatusCode::OK)),
  }
}
