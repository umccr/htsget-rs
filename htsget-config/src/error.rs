//! Error types used by this crate.
//!

use std::{io, result};

use thiserror::Error;

/// The result type for config.
pub type Result<T> = result::Result<T, Error>;

/// The error type for config.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
  #[error("io found: {0}")]
  IoError(String),

  #[error("failed to parse args found: {0}")]
  ArgParseError(String),

  #[error("failed to setup tracing: {0}")]
  TracingError(String),

  #[error("parse error: {0}")]
  ParseError(String),
}

impl From<Error> for io::Error {
  fn from(error: Error) -> Self {
    io::Error::other(error.to_string())
  }
}

impl From<io::Error> for Error {
  fn from(error: io::Error) -> Self {
    Error::IoError(error.to_string())
  }
}

impl From<serde_json::Error> for Error {
  fn from(err: serde_json::Error) -> Self {
    Error::ParseError(err.to_string())
  }
}
