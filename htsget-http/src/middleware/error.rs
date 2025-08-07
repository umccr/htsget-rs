//! Errors related to middleware.
//!

use crate::HtsGetError;
use std::result;
use thiserror::Error;

/// The result type for middleware errors.
pub type Result<T> = result::Result<T, Error>;

/// The error type for middleware errors.
#[derive(Error, Debug)]
pub enum Error {
  #[error("building auth middleware: {0}")]
  AuthBuilderError(String),
}

impl From<jsonwebtoken::errors::Error> for HtsGetError {
  fn from(err: jsonwebtoken::errors::Error) -> Self {
    Self::InvalidAuthentication(format!("invalid JWT: {err}"))
  }
}
