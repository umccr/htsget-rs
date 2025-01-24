//! Error and result types for htsget-storage.
//!

use htsget_config::types::HtsGetError;
use std::io;
use std::io::ErrorKind;
use std::net::AddrParseError;
use thiserror::Error;

/// The result type for storage.
pub type Result<T> = core::result::Result<T, StorageError>;

/// Storage error type.
#[derive(Error, Debug)]
pub enum StorageError {
  #[error("wrong key derived from ID: `{0}`")]
  InvalidKey(String),

  #[error("key not found in storage: `{0}`")]
  KeyNotFound(String),

  #[error("`{0}`: `{1}`")]
  IoError(String, io::Error),

  #[error("server error: `{0}`")]
  ServerError(String),

  #[error("`{0}`")]
  InvalidInput(String),

  #[error("invalid uri: `{0}`")]
  InvalidUri(String),

  #[error("invalid address: `{0}`")]
  InvalidAddress(AddrParseError),
  
  #[error("`{0}`")]
  UnsupportedFormat(String),

  #[error("internal error: `{0}`")]
  InternalError(String),

  #[error("response error: `{0}`")]
  ResponseError(String),

  #[cfg(feature = "aws")]
  #[error("aws error: `{0}`, with key: `{1}`")]
  AwsS3Error(String, String),

  #[error("parsing url: `{0}`")]
  UrlParseError(String),
}

impl From<StorageError> for HtsGetError {
  fn from(err: StorageError) -> Self {
    match err {
      err @ StorageError::InvalidInput(_) => Self::InvalidInput(err.to_string()),
      err @ (StorageError::KeyNotFound(_)
      | StorageError::InvalidKey(_)
      | StorageError::ResponseError(_)) => Self::NotFound(err.to_string()),
      err @ StorageError::IoError(_, _) => Self::IoError(err.to_string()),
      err @ StorageError::UnsupportedFormat(_) => Self::UnsupportedFormat(err.to_string()),
      err @ (StorageError::ServerError(_)
      | StorageError::InvalidUri(_)
      | StorageError::InvalidAddress(_)
      | StorageError::InternalError(_)) => Self::InternalError(err.to_string()),
      #[cfg(feature = "aws")]
      err @ StorageError::AwsS3Error(_, _) => Self::IoError(err.to_string()),
      err @ StorageError::UrlParseError(_) => Self::ParseError(err.to_string()),
    }
  }
}

impl From<StorageError> for io::Error {
  fn from(err: StorageError) -> Self {
    match err {
      StorageError::IoError(_, ref io_error) => Self::new(io_error.kind(), err),
      err => Self::new(ErrorKind::Other, err),
    }
  }
}

impl From<io::Error> for StorageError {
  fn from(error: io::Error) -> Self {
    Self::IoError("io error".to_string(), error)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use htsget_config::types::HtsGetError;

  #[test]
  fn htsget_error_from_storage_not_found() {
    let result = HtsGetError::from(StorageError::KeyNotFound("error".to_string()));
    assert!(matches!(result, HtsGetError::NotFound(_)));
  }

  #[test]
  fn htsget_error_from_storage_invalid_key() {
    let result = HtsGetError::from(StorageError::InvalidKey("error".to_string()));
    assert!(matches!(result, HtsGetError::NotFound(_)));
  }
}
