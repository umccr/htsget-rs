use std::net::AddrParseError;
use std::{io, result};

use thiserror::Error;

pub type Result<T> = result::Result<T, Error>;

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
      io::Error::new(io::ErrorKind::Other, error)
    }
  }
}
