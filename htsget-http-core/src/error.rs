use http::StatusCode;
use serde::Serialize;
use thiserror::Error;

use htsget_search::htsget::HtsGetError as HtsGetSearchError;

pub type Result<T> = core::result::Result<T, HtsGetError>;

/// An error type that describes the errors specified in the
/// [HtsGet specification](https://samtools.github.io/hts-specs/htsget.html)
#[derive(Error, Debug, PartialEq)]
pub enum HtsGetError {
  #[error("invalid authentication: {0}")]
  InvalidAuthentication(String),
  #[error("permission denied: {0}")]
  PermissionDenied(String),
  #[error("not found: {0}")]
  NotFound(String),
  #[error("payload too large: {0}")]
  PayloadTooLarge(String),
  #[error("unsupported format: {0}")]
  UnsupportedFormat(String),
  #[error("invalid input: {0}")]
  InvalidInput(String),
  #[error("invalid range: {0}")]
  InvalidRange(String),
  #[error("internal error: {0}")]
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
      err @ HtsGetError::InvalidAuthentication(_) => (err, StatusCode::UNAUTHORIZED),
      err @ HtsGetError::PermissionDenied(_) => (err, StatusCode::FORBIDDEN),
      err @ HtsGetError::NotFound(_) => (err, StatusCode::NOT_FOUND),
      err @ HtsGetError::PayloadTooLarge(_) => (err, StatusCode::PAYLOAD_TOO_LARGE),
      err @ (HtsGetError::UnsupportedFormat(_)
      | HtsGetError::InvalidInput(_)
      | HtsGetError::InvalidRange(_)) => (err, StatusCode::BAD_REQUEST),
      err @ HtsGetError::InternalError(_) => (err, StatusCode::INTERNAL_SERVER_ERROR),
    };

    (WrappedHtsGetError { htsget: JsonHtsGetError {
      error: self.to_string(),
      message: format!("{}", err),
    }}, status_code)
  }
}

impl From<HtsGetSearchError> for HtsGetError {
  fn from(error: HtsGetSearchError) -> Self {
    match error {
      err @ HtsGetSearchError::NotFound(_) => Self::NotFound(format!("{}", err)),
      err @ HtsGetSearchError::UnsupportedFormat(_) => Self::UnsupportedFormat(format!("{}", err)),
      err @ HtsGetSearchError::InvalidInput(_) => Self::InvalidInput(format!("{}", err)),
      err @ HtsGetSearchError::InvalidRange(_) => Self::InvalidRange(format!("{}", err)),
      err @ (HtsGetSearchError::IoError(_) | HtsGetSearchError::ParseError(_)) => Self::NotFound(format!("{}", err)),
      err @ HtsGetSearchError::InternalError(_) => Self::InternalError(format!("{}", err)),
    }
  }
}
