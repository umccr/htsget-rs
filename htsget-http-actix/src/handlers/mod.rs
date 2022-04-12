use actix_web::{Either, http::StatusCode, Responder};

use htsget_http_core::{JsonResponse, Result};
use pretty_json::PrettyJson;

#[cfg(feature = "async")]
pub use crate::handlers::async_handlers::{
  get_service_info_json, reads_service_info, variants_service_info,
};

#[cfg(feature = "async")]
pub mod async_handlers;
pub mod blocking;
#[cfg(feature = "async")]
pub mod get;
#[cfg(feature = "async")]
pub mod post;

mod pretty_json;

/// Handles a response, converting errors to json and using the proper HTTP status code
fn handle_response(response: Result<JsonResponse>) -> Either<impl Responder, impl Responder> {
  match response {
    Err(error) => {
      let (json, status_code) = error.to_json_representation();
      Either::Left(
        PrettyJson(json)
          .customize()
          .with_status(StatusCode::from_u16(status_code).unwrap()),
      )
    }
    Ok(json) => Either::Right(PrettyJson(json).customize().with_status(StatusCode::OK)),
  }
}
