use actix_web::{http::StatusCode, web::Json, Responder};
use serde::Serialize;
use thiserror::Error;

pub type Result<T> = core::result::Result<T, HtsGetError>;

#[derive(Error, Debug, PartialEq)]
pub enum HtsGetError {
  #[error("InvalidAuthentication")]
  InvalidAuthentication(String),
  #[error("PermissionDenied")]
  PermissionDenied(String),
  #[error("NotFound")]
  NotFound(String),
  #[error("PayloadTooLarge")]
  PayloadTooLarge(String),
  #[error("UnsupportedFormat")]
  UnsupportedFormat(String),
  #[error("InvalidInput")]
  InvalidInput(String),
  #[error("InvalidRange")]
  InvalidRange(String),
}

#[derive(Serialize)]
struct JsonHtsGetError {
  error: String,
  message: String,
}

impl HtsGetError {
  pub fn to_json_responder(&self) -> impl Responder {
    let (message, status_code) = match self {
      HtsGetError::InvalidAuthentication(s) => (s, 401),
      HtsGetError::PermissionDenied(s) => (s, 403),
      HtsGetError::NotFound(s) => (s, 404),
      HtsGetError::PayloadTooLarge(s) => (s, 413),
      HtsGetError::UnsupportedFormat(s) => (s, 400),
      HtsGetError::InvalidInput(s) => (s, 400),
      HtsGetError::InvalidRange(s) => (s, 400),
    };
    Json(JsonHtsGetError {
      error: self.to_string(),
      message: message.clone(),
    })
    .with_status(StatusCode::from_u16(status_code as u16).unwrap())
  }
}
