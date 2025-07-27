//! This module contains error and result types for htsget-axum.
//!

use axum::response::{IntoResponse, Response};
use axum_extra::response::ErasedJson;
use std::net::AddrParseError;
use std::{io, result};
use thiserror::Error;

/// The result type for htsget-axum.
pub type Result<T> = result::Result<T, Error>;

/// The error type for htsget-axum
#[derive(Error, Debug)]
pub enum Error {
  #[error("{0}")]
  IoError(#[from] io::Error),

  #[error("server error: {0}")]
  ServerError(String),

  #[error("invalid address: {0}")]
  InvalidAddress(#[from] AddrParseError),
}

impl From<hyper::Error> for Error {
  fn from(error: hyper::Error) -> Self {
    Self::ServerError(error.to_string())
  }
}

impl From<Error> for io::Error {
  fn from(error: Error) -> Self {
    if let Error::IoError(io) = error {
      io
    } else {
      io::Error::other(error)
    }
  }
}

/// The result type for htsget errors.
pub type HtsGetResult<T> = result::Result<T, HtsGetError>;

/// A wrapper around the http HtsGetError for implementing Axum response traits.
#[derive(Debug)]
pub struct HtsGetError(pub htsget_http::HtsGetError);

impl HtsGetError {
  /// Create a permission denied error.
  pub fn permission_denied(err: String) -> HtsGetError {
    htsget_http::HtsGetError::PermissionDenied(err).into()
  }

  /// Create an invalid authentication error.
  pub fn invalid_authentication(err: String) -> HtsGetError {
    htsget_http::HtsGetError::InvalidAuthentication(err).into()
  }

  /// Create a not found error.
  pub fn not_found(err: String) -> HtsGetError {
    htsget_http::HtsGetError::NotFound(err).into()
  }

  /// Create a payload too large error.
  pub fn payload_too_large(err: String) -> HtsGetError {
    htsget_http::HtsGetError::PayloadTooLarge(err).into()
  }

  /// Create an unsupported format error.
  pub fn unsupported_format(err: String) -> HtsGetError {
    htsget_http::HtsGetError::UnsupportedFormat(err).into()
  }

  /// Create an invalid input error.
  pub fn invalid_input(err: String) -> HtsGetError {
    htsget_http::HtsGetError::InvalidInput(err).into()
  }

  /// Create an invalid range error.
  pub fn invalid_range(err: String) -> HtsGetError {
    htsget_http::HtsGetError::InvalidRange(err).into()
  }

  /// Create an internal error.
  pub fn internal_error(err: String) -> HtsGetError {
    htsget_http::HtsGetError::InternalError(err).into()
  }
}

impl IntoResponse for HtsGetError {
  fn into_response(self) -> Response {
    let (json, status_code) = self.0.to_json_representation();
    (status_code, ErasedJson::pretty(json)).into_response()
  }
}

impl From<htsget_http::HtsGetError> for HtsGetError {
  fn from(err: htsget_http::HtsGetError) -> Self {
    Self(err)
  }
}

impl From<jsonwebtoken::errors::Error> for HtsGetError {
  fn from(err: jsonwebtoken::errors::Error) -> Self {
    Self::invalid_authentication(format!("invalid JWT: {err}"))
  }
}
