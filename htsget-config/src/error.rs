use std::{io, result};

use thiserror::Error;

/// The result type for config.
pub type Result<T> = result::Result<T, Error>;

/// The error type for config.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum Error {
  #[error("io found: {0}")]
  IoError(String),
  #[error("failed to parse args found: {0}")]
  ArgParseError(String),
  #[error("failed to setup tracing: {0}")]
  TracingError(String),
  #[error("parse error: {0}")]
  ParseError(String),
  #[error("config error: {0}")]
  ConfigError(String),
}

impl From<Error> for io::Error {
  fn from(error: Error) -> Self {
    io::Error::new(io::ErrorKind::Other, error.to_string())
  }
}
