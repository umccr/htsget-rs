//! Module providing the abstractions needed to read files from an storage
//!

pub mod local;

use std::{cmp::Ordering, path::PathBuf};
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

#[derive(Debug, Clone)]
pub struct BytesRange {
  start: Option<u64>,
  end: Option<u64>,
}

impl BytesRange {
  pub fn new(start: Option<u64>, end: Option<u64>) -> Self {
    Self { start, end }
  }

  pub fn with_start(mut self, start: u64) -> Self {
    self.start = Some(start);
    self
  }

  pub fn with_end(mut self, end: u64) -> Self {
    self.end = Some(end);
    self
  }

  pub fn get_start(&self) -> Option<u64> {
    self.start
  }

  pub fn get_end(&self) -> Option<u64> {
    self.end
  }

  pub fn overlaps(&self, range: &BytesRange) -> bool {
    let cond1 = match (self.start.as_ref(), range.end.as_ref()) {
      (None, None) | (None, Some(_)) | (Some(_), None) => true,
      (Some(start), Some(end)) => end >= start,
    };
    let cond2 = match (self.end.as_ref(), range.start.as_ref()) {
      (None, None) | (None, Some(_)) | (Some(_), None) => true,
      (Some(end), Some(start)) => end >= start,
    };
    cond1 && cond2
  }

  pub fn merge_with(&mut self, range: &BytesRange) {
    self.start = match (self.start.as_ref(), range.start.as_ref()) {
      (None, None) | (None, Some(_)) | (Some(_), None) => None,
      (Some(a), Some(b)) => Some(*a.min(b)),
    };
    self.end = match (self.end.as_ref(), range.end.as_ref()) {
      (None, None) | (None, Some(_)) | (Some(_), None) => None,
      (Some(a), Some(b)) => Some(*a.max(b)),
    };
  }

  pub fn merge_all(mut ranges: Vec<BytesRange>) -> Vec<BytesRange> {
    if ranges.len() < 2 {
      ranges
    } else {
      ranges.sort_by(|a, b| {
        let a_start = a.get_start().unwrap_or(0);
        let b_start = b.get_start().unwrap_or(0);
        let start_ord = a_start.cmp(&b_start);
        if start_ord == Ordering::Equal {
          let a_end = a.get_end().unwrap_or(u64::MAX);
          let b_end = b.get_end().unwrap_or(u64::MAX);
          b_end.cmp(&a_end)
        } else {
          start_ord
        }
      });

      let mut optimized_ranges = Vec::with_capacity(ranges.len());

      let mut current_range = ranges[0].clone();

      for range in ranges.iter().skip(1) {
        if current_range.overlaps(range) {
          current_range.merge_with(range)
        } else {
          optimized_ranges.push(current_range.clone());
          current_range = range.clone();
        }
      }

      optimized_ranges.push(current_range.clone());

      optimized_ranges
    }
  }
}

impl Default for BytesRange {
  fn default() -> Self {
    Self {
      start: None,
      end: None,
    }
  }
}

pub struct GetOptions {
  range: BytesRange,
}

impl GetOptions {
  pub fn with_max_length(mut self, max_length: u64) -> Self {
    self.range = BytesRange::default().with_start(0).with_end(max_length);
    self
  }

  pub fn with_range(mut self, range: BytesRange) -> Self {
    self.range = range;
    self
  }
}

impl Default for GetOptions {
  fn default() -> Self {
    Self {
      range: BytesRange::default(),
    }
  }
}

pub struct UrlOptions {
  range: BytesRange,
  class: Option<Class>,
}

impl UrlOptions {
  pub fn with_range(mut self, range: BytesRange) -> Self {
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
      range: BytesRange::default(),
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
