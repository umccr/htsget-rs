use http::StatusCode;
use http::header::{InvalidHeaderName, InvalidHeaderValue};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::HtsGetError::InternalError;
use htsget_config::types::HtsGetError as HtsGetSearchError;

pub type Result<T> = core::result::Result<T, HtsGetError>;

/// An error type that describes the errors specified in the
/// [HtsGet specification](https://samtools.github.io/hts-specs/htsget.html)
#[derive(Error, Debug, PartialEq, Eq)]
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
  #[error("MethodNotAllowed")]
  MethodNotAllowed(String),
  #[error("InternalError")]
  InternalError(String),
  #[error("Wrapped")]
  Wrapped(WrappedHtsGetError, StatusCode),
}

/// A helper struct implementing [serde's Serialize trait](Serialize) to allow
/// easily converting HtsGetErrors to JSON
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct JsonHtsGetError {
  error: String,
  message: String,
}

/// The "htsget" container wrapping the actual error response above
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct WrappedHtsGetError {
  htsget: JsonHtsGetError,
}

impl HtsGetError {
  /// Allows converting the error to JSON and the correspondent
  /// status code
  pub fn to_json_representation(&self) -> (WrappedHtsGetError, StatusCode) {
    let (err, status_code) = match self {
      HtsGetError::InvalidAuthentication(err) => (err, StatusCode::UNAUTHORIZED),
      HtsGetError::PermissionDenied(err) => (err, StatusCode::FORBIDDEN),
      HtsGetError::NotFound(err) => (err, StatusCode::NOT_FOUND),
      HtsGetError::PayloadTooLarge(err) => (err, StatusCode::PAYLOAD_TOO_LARGE),
      HtsGetError::UnsupportedFormat(err)
      | HtsGetError::InvalidInput(err)
      | HtsGetError::InvalidRange(err) => (err, StatusCode::BAD_REQUEST),
      HtsGetError::MethodNotAllowed(err) => (err, StatusCode::METHOD_NOT_ALLOWED),
      HtsGetError::InternalError(err) => (err, StatusCode::INTERNAL_SERVER_ERROR),
      HtsGetError::Wrapped(err, status) => return (err.clone(), *status),
    };

    (
      WrappedHtsGetError {
        htsget: JsonHtsGetError {
          error: self.to_string(),
          message: err.to_string(),
        },
      },
      status_code,
    )
  }
}

impl From<HtsGetSearchError> for HtsGetError {
  fn from(error: HtsGetSearchError) -> Self {
    match error {
      HtsGetSearchError::NotFound(err) => Self::NotFound(err),
      HtsGetSearchError::UnsupportedFormat(err) => Self::UnsupportedFormat(err),
      HtsGetSearchError::InvalidInput(err) => Self::InvalidInput(err),
      HtsGetSearchError::InvalidRange(err) => Self::InvalidRange(err),
      HtsGetSearchError::IoError(err) | HtsGetSearchError::ParseError(err) => Self::NotFound(err),
      HtsGetSearchError::InternalError(err) => Self::InternalError(err),
    }
  }
}

impl From<InvalidHeaderName> for HtsGetError {
  fn from(err: InvalidHeaderName) -> Self {
    Self::InternalError(err.to_string())
  }
}

impl From<InvalidHeaderValue> for HtsGetError {
  fn from(err: InvalidHeaderValue) -> Self {
    Self::InternalError(err.to_string())
  }
}

impl From<reqwest_middleware::Error> for HtsGetError {
  fn from(err: reqwest_middleware::Error) -> Self {
    match err {
      reqwest_middleware::Error::Middleware(err) => InternalError(err.to_string()),
      reqwest_middleware::Error::Reqwest(err) => err
        .status()
        .map(|status| match status {
          StatusCode::UNAUTHORIZED => HtsGetError::InvalidAuthentication(err.to_string()),
          StatusCode::FORBIDDEN => HtsGetError::PermissionDenied(err.to_string()),
          StatusCode::NOT_FOUND => HtsGetError::NotFound(err.to_string()),
          StatusCode::PAYLOAD_TOO_LARGE => HtsGetError::PayloadTooLarge(err.to_string()),
          StatusCode::BAD_REQUEST => HtsGetError::InvalidInput(err.to_string()),
          StatusCode::METHOD_NOT_ALLOWED => HtsGetError::MethodNotAllowed(err.to_string()),
          _ => HtsGetError::InternalError(err.to_string()),
        })
        .unwrap_or(HtsGetError::InternalError(err.to_string())),
    }
  }
}
