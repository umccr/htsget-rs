use thiserror::Error;

pub type Result<T> = core::result::Result<T, HtsGetError>;

#[derive(Error, Debug, PartialEq)]
pub enum HtsGetError {
  #[error("Invalid ID: {0}")]
  InvalidAuthentication(String),
  #[error("Invalid ID: {0}")]
  PermissionDenied(String),
  #[error("Invalid ID: {0}")]
  NotFound(String),
  #[error("Invalid ID: {0}")]
  PayloadTooLarge(String),
  #[error("Invalid ID: {0}")]
  UnsupportedFormat(String),
  #[error("Invalid ID: {0}")]
  InvalidInput(String),
  #[error("Invalid ID: {0}")]
  InvalidRange(String),
}
