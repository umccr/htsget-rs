//! Module providing a representation of the HtsGet specification.
//!
//! Based on the [HtsGet Specification](https://samtools.github.io/hts-specs/htsget.html).
//!

pub mod bam_search;
pub mod from_storage;

use std::collections::HashMap;

use thiserror::Error;

use crate::storage::StorageError;

type Result<T> = core::result::Result<T, HtsGetError>;

/// Trait representing a search for either `reads` or `variants` in the HtsGet specification.
pub trait HtsGet {
  fn search(&self, query: Query) -> Result<Response>;
}

#[derive(Error, Debug, PartialEq)]
pub enum HtsGetError {
  #[error("Not found: {0}")]
  NotFound(String),

  #[error("Unsupported Format: {0}")]
  UnsupportedFormat(String),

  #[error("Invalid input: {0}")]
  InvalidInput(String),

  #[error("Invalid range: {0}")]
  InvalidRange(String),

  #[error("IO error: {0}")]
  IoError(String),
}

impl HtsGetError {
  pub fn not_found<S: Into<String>>(message: S) -> Self {
    Self::NotFound(message.into())
  }

  pub fn unsupported_format<S: Into<String>>(format: S) -> Self {
    Self::UnsupportedFormat(format.into())
  }

  pub fn invalid_input<S: Into<String>>(message: S) -> Self {
    Self::InvalidInput(message.into())
  }

  pub fn invalid_range<S: Into<String>>(message: S) -> Self {
    Self::InvalidRange(message.into())
  }

  pub fn io_error<S: Into<String>>(message: S) -> Self {
    Self::IoError(message.into())
  }
}

impl From<StorageError> for HtsGetError {
  fn from(err: StorageError) -> Self {
    match err {
      StorageError::NotFound(key) => Self::NotFound(format!("Not found in storage: {}", key)),
      StorageError::InvalidKey(key) => {
        Self::InvalidInput(format!("Wrong key derived from ID: {}", key))
      }
    }
  }
}

/// A query contains all the parameters that can be used when requesting
/// a search for either of `reads` or `variants`.
#[derive(Debug)]
pub struct Query {
  pub id: String,
  pub format: Option<Format>,
  pub class: Option<Class>,
  /// Reference name
  pub reference_name: Option<String>,
  /// sequence start position (1-based)
  pub start: Option<u32>,
  /// sequence end position (1-based)
  pub end: Option<u32>,
  pub fields: Vec<String>,
  pub tags: Option<Tags>,
  pub no_tags: Option<Vec<String>>,
}

impl Query {
  pub fn new(id: impl Into<String>) -> Self {
    Self {
      id: id.into(),
      format: None,
      class: None,
      reference_name: None,
      start: None,
      end: None,
      fields: Vec::new(),
      tags: None,
      no_tags: None,
    }
  }

  pub fn with_format(mut self, format: Format) -> Self {
    self.format = Some(format);
    self
  }

  pub fn with_class(mut self, class: Class) -> Self {
    self.class = Some(class);
    self
  }

  pub fn with_reference_name(mut self, reference_name: impl Into<String>) -> Self {
    self.reference_name = Some(reference_name.into());
    self
  }

  pub fn with_start(mut self, start: u32) -> Self {
    self.start = Some(start);
    self
  }

  pub fn with_end(mut self, end: u32) -> Self {
    self.end = Some(end);
    self
  }

  // TODO the rest of the builder methods ...
}

/// An enumeration with all the possible formats.
#[derive(Debug, PartialEq)]
pub enum Format {
  Bam,
  Cram,
  Vcf,
  Bcf,
}

impl From<Format> for String {
  fn from(format: Format) -> Self {
    match format {
      Format::Bam => "BAM",
      Format::Cram => "CRAM",
      Format::Vcf => "VCF",
      Format::Bcf => "BCF",
    }
    .to_string()
  }
}

#[derive(Debug, PartialEq)]
pub enum Class {
  Header,
  Body,
}

/// Possible values for the tags parameter.
#[derive(Debug)]
pub enum Tags {
  /// Include all tags
  All,
  /// List of tags to include
  List(Vec<String>),
}

/// The headers that need to be supplied when requesting data from a url.
#[derive(Debug, PartialEq)]
pub struct Headers(HashMap<String, String>);

impl Headers {
  pub fn new(headers: HashMap<String, String>) -> Self {
    Self(headers)
  }

  pub fn with_header<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
    self.0.insert(key.into(), value.into());
    self
  }

  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  pub fn insert<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) {
    self.0.insert(key.into(), value.into());
  }
}

impl Default for Headers {
  fn default() -> Self {
    Self(HashMap::new())
  }
}

/// A url from which raw data can be retrieved.
#[derive(Debug, PartialEq)]
pub struct Url {
  pub url: String,
  pub headers: Option<Headers>,
  pub class: Option<Class>,
}

impl Url {
  pub fn new<S: Into<String>>(url: S) -> Self {
    Self {
      url: url.into(),
      headers: None,
      class: None,
    }
  }

  pub fn with_headers(mut self, headers: Headers) -> Self {
    self.headers = Some(headers).filter(|h| !h.is_empty());
    self
  }

  pub fn with_class(mut self, class: Class) -> Self {
    self.class = Some(class);
    self
  }
}

/// The response for a HtsGet query.
#[derive(Debug, PartialEq)]
pub struct Response {
  pub format: Format,
  pub urls: Vec<Url>,
}

impl Response {
  pub fn new(format: Format, urls: Vec<Url>) -> Self {
    Self { format, urls }
  }
}
