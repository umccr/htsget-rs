//! Module providing the abstractions needed to read files from an storage
//!

pub mod local;

use std::path::PathBuf;
use thiserror::Error;

use crate::htsget::{Class, Url};

#[derive(Error, PartialEq, Debug)]
pub enum StorageError {
  #[error("Invalid key: {0}")]
  InvalidKey(String),

  #[error("Not found: {0}")]
  NotFound(String),
}

type Result<T> = core::result::Result<T, StorageError>;

pub struct Range {
  start: Option<u64>,
  end: Option<u64>,
}

impl Range {
  pub fn new(start: Option<u64>, end: Option<u64>) -> Self {
    Self {
      start,
      end,
    }
  }

  pub fn with_start(mut self, start: u64) -> Self {
    self.start = Some(start);
    self
  }

  pub fn with_end(mut self, end: u64) -> Self {
    self.end = Some(end);
    self
  }
}

impl Default for Range {
  fn default() -> Self {
    Self {
      start: None,
      end: None,
    }
  }
}

pub struct GetOptions {
  range: Range,
}

impl GetOptions {
  pub fn with_max_length(mut self, max_length: u64) -> Self {
    self.range = Range::default().with_start(0).with_end(max_length);
    self
  }

  pub fn with_range(mut self, range: Range) -> Self {
    self.range = range;
    self
  }
}

impl Default for GetOptions {
  fn default() -> Self {
    Self {
      range: Range::default(),
    }
  }
}

pub struct UrlOptions {
  range: Range,
  class: Option<Class>,
}

impl UrlOptions {
  pub fn with_range(mut self, range: Range) -> Self {
    self.range = range;
    self
  }

  pub fn with_class(mut self, class: Class) -> Self {
    self.class = Some(class);
    self
  }
}

impl Default for UrlOptions {
  fn default() -> Self {
    Self {
      range: Range::default(),
      class: None,
    }
  }
}

/// An Storage represents some kind of object based storage (either locally or in the cloud)
/// that can be used to retrieve files for alignments, variants or its respective indexes.
pub trait Storage {
  // TODO Consider another type of interface based on IO streaming
  // so we don't need to guess the length of the headers, but just
  // parse them in an streaming fashion.
  fn get<K: AsRef<str>>(&self, key: K, options: GetOptions) -> Result<PathBuf>;

  fn url<K: AsRef<str>>(&self, key: K, options: UrlOptions) -> Result<Url>;
}
