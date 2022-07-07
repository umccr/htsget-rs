use http::StatusCode;
use serde::Serialize;
use thiserror::Error;

use htsget_search::htsget::HtsGetError as HtsGetSearchError;

pub type Result<T> = core::result::Result<T, HtsGetError>;

/// An error type that describes the errors specified in the
/// [HtsGet specification](https://samtools.github.io/hts-specs/htsget.html)
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
  #[error("Internal error")]
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
pub struct ContainerHtsGetError {
  htsget: JsonHtsGetError,
}

impl HtsGetError {
  /// Allows converting the error to JSON and the correspondent
  /// status code
  pub fn to_json_representation(&self) -> (ContainerHtsGetError, StatusCode) {
    let (message, status_code) = match self {
      HtsGetError::InvalidAuthentication(s) => (s, StatusCode::UNAUTHORIZED),
      HtsGetError::PermissionDenied(s) => (s, StatusCode::FORBIDDEN),
      HtsGetError::NotFound(s) => (s, StatusCode::NOT_FOUND),
      HtsGetError::PayloadTooLarge(s) => (s, StatusCode::PAYLOAD_TOO_LARGE),
      HtsGetError::UnsupportedFormat(s)
      | HtsGetError::InvalidInput(s)
      | HtsGetError::InvalidRange(s) => (s, StatusCode::BAD_REQUEST),
      HtsGetError::InternalError(s) => (s, StatusCode::INTERNAL_SERVER_ERROR),
    };

    // Actual error and message...
    let inner_json = JsonHtsGetError {
      error: self.to_string(),
      message: message.clone(),
    };

    // ...and "htsget" wrapping
    (ContainerHtsGetError { htsget: inner_json }, status_code)
  }
}

impl From<HtsGetSearchError> for HtsGetError {
  fn from(error: HtsGetSearchError) -> Self {
    match error {
      HtsGetSearchError::NotFound(s) => Self::NotFound(s),
      HtsGetSearchError::UnsupportedFormat(s) => Self::UnsupportedFormat(s),
      HtsGetSearchError::InvalidInput(s) => Self::InvalidInput(s),
      HtsGetSearchError::InvalidRange(s) => Self::InvalidRange(s),
      HtsGetSearchError::IoError(s) => Self::NotFound(format!("There was an IO error: {}", s)),
      HtsGetSearchError::ParseError(s) => Self::NotFound(format!(
        "The requested content couldn't be parsed correctly {}",
        s
      )),
      HtsGetSearchError::InternalError(s) => Self::InternalError(s),
    }
  }
}
