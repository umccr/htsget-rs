use actix_web::{http::StatusCode, web::Json, Responder};
use htsget_http_core::Result;

pub mod get;
pub mod post;

fn handle_response(response: Result<String>) -> impl Responder {
  match response {
    Err(error) => {
      let (json, status_code) = error.to_json_representation();
      Json(json).with_status(StatusCode::from_u16(status_code).unwrap())
    }
    Ok(json) => Json(json).with_status(StatusCode::OK),
  }
}
