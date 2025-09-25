use http::StatusCode;
use serde::Serialize;
use thiserror::Error;

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
}

/// A helper struct implementing [serde's Serialize trait](Serialize) to allow
/// easily converting HtsGetErrors to JSON
#[derive(Serialize)]
pub struct JsonHtsGetError {
  error: String,
  message: String,
}

/// The "htsget" container wrapping the actual error response above
#[derive(Serialize)]
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
