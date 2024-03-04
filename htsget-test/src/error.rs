//! Errors defined by the htsget-test crate.
//!

use std::fmt::Display;
use std::io::Error;
use std::{io, result};
use thiserror::Error;

/// Result type for this crate.
pub type Result<T> = result::Result<T, TestError>;

/// The error that this crate can make.
#[derive(Error, Debug)]
pub enum TestError {
  #[error("{0}")]
  Io(io::Error),
  #[error("reading records: {0}")]
  ReadRecord(String),
  #[error("concatenating response: {0}")]
  ConcatResponse(String),
}

impl TestError {
  /// Create a read record error.
  pub fn read_record<E: Display>(error: E) -> Self {
    Self::ReadRecord(error.to_string())
  }

  /// Create a concat response error.
  pub fn concat_response<E: Display>(error: E) -> Self {
    Self::ConcatResponse(error.to_string())
  }
}

impl From<io::Error> for TestError {
  fn from(error: Error) -> Self {
    Self::Io(error)
  }
}

impl From<TestError> for io::Error {
  fn from(error: TestError) -> Self {
    Error::new(io::ErrorKind::Other, error)
  }
}
