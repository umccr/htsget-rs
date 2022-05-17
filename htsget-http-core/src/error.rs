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

impl HtsGetError {
  /// Allows converting the error to JSON and the correspondent
  /// status code
  pub fn to_json_representation(&self) -> (JsonHtsGetError, u16) {
    let (message, status_code) = match self {
      HtsGetError::InvalidAuthentication(s) => (s, 401),
      HtsGetError::PermissionDenied(s) => (s, 403),
      HtsGetError::NotFound(s) => (s, 404),
      HtsGetError::PayloadTooLarge(s) => (s, 413),
      HtsGetError::UnsupportedFormat(s) => (s, 400),
      HtsGetError::InvalidInput(s) => (s, 400),
      HtsGetError::InvalidRange(s) => (s, 400),
      HtsGetError::InternalError(s) => (s, 500),
    };
    (
      JsonHtsGetError {
        error: self.to_string(),
        message: message.clone(),
      },
      status_code,
    )
  }
}

impl From<HtsGetSearchError> for HtsGetError {
  fn from(error: HtsGetSearchError) -> Self {
    match error {
      HtsGetSearchError::NotFound(s) => HtsGetError::NotFound(s),
      HtsGetSearchError::UnsupportedFormat(s) => HtsGetError::UnsupportedFormat(s),
      HtsGetSearchError::InvalidInput(s) => HtsGetError::InvalidInput(s),
      HtsGetSearchError::InvalidRange(s) => HtsGetError::InvalidRange(s),
      HtsGetSearchError::IoError(s) => {
        HtsGetError::NotFound(format!("There was an IO error: {}", s))
      }
      HtsGetSearchError::ParseError(s) => HtsGetError::NotFound(format!(
        "The requested content couldn't be parsed correctly {}",
        s
      )),
      HtsGetSearchError::InternalError(s) => HtsGetError::InternalError(s),
    }
  }
}
