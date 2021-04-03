//! Module providing the abstractions needed to read files from an storage
//!

pub mod local;

use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
  #[error("Invalid key: {0}")]
  InvalidKey(String),

  #[error("Not found: {0}")]
  NotFound(String),
}

type Result<T> = core::result::Result<T, StorageError>;

pub struct GetOptions {
  max_length: Option<usize>,
}

impl GetOptions {
  pub fn with_max_length(mut self, max_length: usize) -> Self {
    self.max_length = Some(max_length);
    self
  }
}

impl Default for GetOptions {
  fn default() -> Self {
    Self { max_length: None }
  }
}

/// An Storage represents some kind of object based storage (either locally or in the cloud)
/// that can be used to retrieve files for alignments, variants or its respective indexes.
pub trait Storage {
  // TODO Consider another type of interface based on IO streaming
  // so we don't need to guess the length of the headers, but just
  // parse them in an streaming fashion.
  fn get<K: AsRef<str>>(&self, key: K, options: GetOptions) -> Result<PathBuf>;
}
