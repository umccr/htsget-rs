/// HtsGet model and interface
///
/// Based on the htsget spec: https://samtools.github.io/hts-specs/htsget.html
///
use thiserror::Error;

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
  IOError(String),
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
    Self::IOError(message.into())
  }
}

#[derive(Debug)]
pub struct Query {
  pub id: String,
  pub format: Option<Format>,
  pub class: Option<String>,
  pub reference_name: Option<String>,
  pub start: Option<u32>,
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

  pub fn with_class(mut self, class: impl Into<String>) -> Self {
    self.class = Some(class.into());
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

#[derive(Debug)]
pub enum Format {
  BAM,
  CRAM,
  VCF,
  BCF,
}

impl Into<String> for Format {
  fn into(self) -> String {
    match self {
      Self::BAM => "BAM",
      Self::CRAM => "CRAM",
      Self::VCF => "VCF",
      Self::BCF => "BCF",
    }
    .to_string()
  }
}

#[derive(Debug)]
pub enum Tags {
  All,
  List(Vec<String>),
}

#[derive(Debug)]
pub struct Headers {
  pub authorization: String,
  pub range: String,
}

impl Headers {
  pub fn new(authorization: String, range: String) -> Self {
    Self {
      authorization,
      range,
    }
  }
}

#[derive(Debug)]
pub struct Url {
  pub url: String,
  pub headers: Option<Headers>,
  pub class: Option<String>,
}

impl Url {
  pub fn new(url: String, headers: Option<Headers>, class: Option<String>) -> Self {
    Self {
      url,
      headers,
      class,
    }
  }
}

#[derive(Debug)]
pub struct Response {
  pub format: Format,
  pub urls: Vec<Url>,
}

pub trait HtsGet {
  fn search(&self, query: Query) -> Result<Response, HtsGetError>;
}
