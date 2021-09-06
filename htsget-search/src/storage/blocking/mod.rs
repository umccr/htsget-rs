//! Module providing the abstractions needed to read files from an storage
//!
use std::path::PathBuf;

use crate::htsget::Url;
use crate::storage::{GetOptions, StorageError, UrlOptions};

pub mod local;
#[cfg(feature = "aws")]
pub mod aws;

type Result<T> = core::result::Result<T, StorageError>;

/// A Storage represents some kind of object based storage (either locally or in the cloud)
/// that can be used to retrieve files for alignments, variants or its respective indexes.
pub trait Storage {
  // TODO Consider another type of interface based on IO streaming
  // so we don't need to guess the length of the headers, but just
  // parse them in an streaming fashion.
  fn get<K: AsRef<str>>(&self, key: K, options: GetOptions) -> Result<PathBuf>;

  fn url<K: AsRef<str>>(&self, key: K, options: UrlOptions) -> Result<Url>;

  fn head<K: AsRef<str>>(&self, key: K) -> Result<u64>;
}
