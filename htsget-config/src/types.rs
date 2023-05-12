use std::collections::{HashMap, HashSet};
use std::fmt::Formatter;
use std::io::ErrorKind;
use std::io::ErrorKind::Other;
use std::{fmt, io, result};

use noodles::core::region::Interval as NoodlesInterval;
use noodles::core::Position;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::instrument;

pub type Result<T> = result::Result<T, HtsGetError>;

/// An enumeration with all the possible formats.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "UPPERCASE"))]
pub enum Format {
  #[serde(alias = "bam", alias = "BAM")]
  Bam,
  #[serde(alias = "cram", alias = "CRAM")]
  Cram,
  #[serde(alias = "vcf", alias = "VCF")]
  Vcf,
  #[serde(alias = "bcf", alias = "BCF")]
  Bcf,
}

/// Todo allow these to be configurable.
impl Format {
  pub fn fmt_file(&self, id: &str) -> String {
    match self {
      Format::Bam => format!("{id}.bam"),
      Format::Cram => format!("{id}.cram"),
      Format::Vcf => format!("{id}.vcf.gz"),
      Format::Bcf => format!("{id}.bcf"),
    }
  }

  pub fn fmt_index(&self, id: &str) -> String {
    match self {
      Format::Bam => format!("{id}.bam.bai"),
      Format::Cram => format!("{id}.cram.crai"),
      Format::Vcf => format!("{id}.vcf.gz.tbi"),
      Format::Bcf => format!("{id}.bcf.csi"),
    }
  }

  pub fn fmt_gzi(&self, id: &str) -> io::Result<String> {
    match self {
      Format::Bam => Ok(format!("{id}.bam.gzi")),
      Format::Cram => Err(io::Error::new(
        Other,
        "CRAM does not support GZI".to_string(),
      )),
      Format::Vcf => Ok(format!("{id}.vcf.gz.gzi")),
      Format::Bcf => Ok(format!("{id}.bcf.gzi")),
    }
  }
}

impl From<Format> for String {
  fn from(format: Format) -> Self {
    format.to_string()
  }
}

impl fmt::Display for Format {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Format::Bam => write!(f, "BAM"),
      Format::Cram => write!(f, "CRAM"),
      Format::Vcf => write!(f, "VCF"),
      Format::Bcf => write!(f, "BCF"),
    }
  }
}

/// Class component of htsget response.
#[derive(Copy, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "lowercase"))]
pub enum Class {
  #[serde(alias = "header", alias = "HEADER")]
  Header,
  #[serde(alias = "body", alias = "BODY")]
  Body,
}

/// An interval represents the start (0-based, inclusive) and end (0-based exclusive) ranges of the
/// query.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Interval {
  start: Option<u32>,
  end: Option<u32>,
}

impl Interval {
  /// Check if this interval contains the value.
  pub fn contains(&self, value: u32) -> bool {
    return match (self.start.as_ref(), self.end.as_ref()) {
      (None, None) => true,
      (None, Some(end)) => value < *end,
      (Some(start), None) => value >= *start,
      (Some(start), Some(end)) => value >= *start && value < *end,
    };
  }

  /// Convert this interval into a one-based noodles `Interval`.
  #[instrument(level = "trace", skip_all, ret)]
  pub fn into_one_based(self) -> io::Result<NoodlesInterval> {
    Ok(match (self.start, self.end) {
      (None, None) => NoodlesInterval::from(..),
      (None, Some(end)) => NoodlesInterval::from(..=Self::convert_end(end)?),
      (Some(start), None) => NoodlesInterval::from(Self::convert_start(start)?..),
      (Some(start), Some(end)) => {
        NoodlesInterval::from(Self::convert_start(start)?..=Self::convert_end(end)?)
      }
    })
  }

  /// Convert a start position to a noodles Position.
  pub fn convert_start(start: u32) -> io::Result<Position> {
    Self::convert_position(start, |value| {
      value.checked_add(1).ok_or_else(|| {
        io::Error::new(
          Other,
          format!("could not convert {value} to 1-based position."),
        )
      })
    })
  }

  /// Convert an end position to a noodles Position.
  pub fn convert_end(end: u32) -> io::Result<Position> {
    Self::convert_position(end, Ok)
  }

  /// Convert a u32 position to a noodles Position.
  pub fn convert_position<F>(value: u32, convert_fn: F) -> io::Result<Position>
  where
    F: FnOnce(u32) -> io::Result<u32>,
  {
    let value = convert_fn(value).map(|value| {
      usize::try_from(value)
        .map_err(|err| io::Error::new(Other, format!("could not convert `u32` to `usize`: {err}")))
    })??;

    Position::try_from(value).map_err(|err| {
      io::Error::new(
        Other,
        format!("could not convert `{value}` into `Position`: {err}"),
      )
    })
  }

  pub fn start(&self) -> Option<u32> {
    self.start
  }

  pub fn end(&self) -> Option<u32> {
    self.end
  }

  /// Create a new interval
  pub fn new(start: Option<u32>, end: Option<u32>) -> Self {
    Self { start, end }
  }
}

/// Schemes that can be used with htsget.
#[derive(Serialize, Deserialize, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Scheme {
  #[default]
  #[serde(alias = "Http", alias = "http")]
  Http,
  #[serde(alias = "Https", alias = "https")]
  Https,
}

/// Tagged Any allow type for cors config.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TaggedTypeAll {
  #[serde(alias = "all", alias = "ALL")]
  All,
}

/// Possible values for the fields parameter.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Fields {
  /// Include all fields
  Tagged(TaggedTypeAll),
  /// List of fields to include
  List(HashSet<String>),
}

/// Possible values for the tags parameter.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Tags {
  /// Include all tags
  Tagged(TaggedTypeAll),
  /// List of tags to include
  List(HashSet<String>),
}

/// The no tags parameter.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct NoTags(pub Option<HashSet<String>>);

/// A query contains all the parameters that can be used when requesting
/// a search for either of `reads` or `variants`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
  id: String,
  format: Format,
  class: Class,
  /// Reference name
  reference_name: Option<String>,
  /// The start and end positions are 0-based. [start, end)
  interval: Interval,
  fields: Fields,
  tags: Tags,
  no_tags: NoTags,
}

impl Query {
  pub fn new(id: impl Into<String>, format: Format) -> Self {
    Self {
      id: id.into(),
      format,
      class: Class::Body,
      reference_name: None,
      interval: Interval::default(),
      fields: Fields::Tagged(TaggedTypeAll::All),
      tags: Tags::Tagged(TaggedTypeAll::All),
      no_tags: NoTags(None),
    }
  }

  pub fn set_id(&mut self, id: impl Into<String>) {
    self.id = id.into();
  }

  pub fn with_id(mut self, id: impl Into<String>) -> Self {
    self.set_id(id);
    self
  }

  pub fn with_format(mut self, format: Format) -> Self {
    self.format = format;
    self
  }

  pub fn with_class(mut self, class: Class) -> Self {
    self.class = class;
    self
  }

  pub fn with_reference_name(mut self, reference_name: impl Into<String>) -> Self {
    self.reference_name = Some(reference_name.into());
    self
  }

  pub fn with_start(mut self, start: u32) -> Self {
    self.interval.start = Some(start);
    self
  }

  pub fn with_end(mut self, end: u32) -> Self {
    self.interval.end = Some(end);
    self
  }

  pub fn with_fields(mut self, fields: Fields) -> Self {
    self.fields = fields;
    self
  }

  pub fn with_tags(mut self, tags: Tags) -> Self {
    self.tags = tags;
    self
  }

  pub fn with_no_tags(mut self, no_tags: Vec<impl Into<String>>) -> Self {
    self.no_tags = NoTags(Some(
      no_tags.into_iter().map(|field| field.into()).collect(),
    ));
    self
  }

  pub fn id(&self) -> &str {
    &self.id
  }

  pub fn format(&self) -> Format {
    self.format
  }

  pub fn class(&self) -> Class {
    self.class
  }

  pub fn reference_name(&self) -> Option<&str> {
    self.reference_name.as_deref()
  }

  pub fn interval(&self) -> Interval {
    self.interval
  }

  pub fn fields(&self) -> &Fields {
    &self.fields
  }

  pub fn tags(&self) -> &Tags {
    &self.tags
  }

  pub fn no_tags(&self) -> &NoTags {
    &self.no_tags
  }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum HtsGetError {
  #[error("not found: {0}")]
  NotFound(String),

  #[error("unsupported Format: {0}")]
  UnsupportedFormat(String),

  #[error("invalid input: {0}")]
  InvalidInput(String),

  #[error("invalid range: {0}")]
  InvalidRange(String),

  #[error("io error: {0}")]
  IoError(String),

  #[error("parsing error: {0}")]
  ParseError(String),

  #[error("internal error: {0}")]
  InternalError(String),
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

  pub fn parse_error<S: Into<String>>(message: S) -> Self {
    Self::ParseError(message.into())
  }

  pub fn internal_error<S: Into<String>>(message: S) -> Self {
    Self::InternalError(message.into())
  }
}

impl From<HtsGetError> for io::Error {
  fn from(error: HtsGetError) -> Self {
    Self::new(ErrorKind::Other, error)
  }
}

impl From<io::Error> for HtsGetError {
  fn from(err: io::Error) -> Self {
    Self::io_error(err.to_string())
  }
}

/// The headers that need to be supplied when requesting data from a url.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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

  pub fn into_inner(self) -> HashMap<String, String> {
    self.0
  }

  pub fn as_ref_inner(&self) -> &HashMap<String, String> {
    &self.0
  }
}

/// A url from which raw data can be retrieved.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Url {
  pub url: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub headers: Option<Headers>,
  #[serde(skip_serializing_if = "Option::is_none")]
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

  pub fn set_class(mut self, class: Option<Class>) -> Self {
    self.class = class;
    self
  }

  pub fn with_class(self, class: Class) -> Self {
    self.set_class(Some(class))
  }
}

/// Wrapped json response for htsget.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonResponse {
  pub htsget: Response,
}

impl JsonResponse {
  pub fn new(htsget: Response) -> Self {
    Self { htsget }
  }
}

impl From<Response> for JsonResponse {
  fn from(htsget: Response) -> Self {
    Self::new(htsget)
  }
}

/// The response for a HtsGet query.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
  pub format: Format,
  pub urls: Vec<Url>,
}

impl Response {
  pub fn new(format: Format, urls: Vec<Url>) -> Self {
    Self { format, urls }
  }
}

#[cfg(test)]
mod tests {
  use std::collections::{HashMap, HashSet};

  use crate::types::{
    Class, Fields, Format, Headers, HtsGetError, Interval, NoTags, Query, Response, TaggedTypeAll,
    Tags, Url,
  };

  #[test]
  fn interval_contains() {
    let interval = Interval {
      start: Some(0),
      end: Some(10),
    };
    assert!(interval.contains(9));
  }

  #[test]
  fn interval_not_contains() {
    let interval = Interval {
      start: Some(0),
      end: Some(10),
    };
    assert!(!interval.contains(10));
  }

  #[test]
  fn interval_contains_start_not_present() {
    let interval = Interval {
      start: None,
      end: Some(10),
    };
    assert!(interval.contains(9));
  }

  #[test]
  fn interval_not_contains_start_not_present() {
    let interval = Interval {
      start: None,
      end: Some(10),
    };
    assert!(!interval.contains(10));
  }

  #[test]
  fn interval_contains_end_not_present() {
    let interval = Interval {
      start: Some(1),
      end: None,
    };
    assert!(interval.contains(9));
  }

  #[test]
  fn interval_not_contains_end_not_present() {
    let interval = Interval {
      start: Some(1),
      end: None,
    };
    assert!(!interval.contains(0));
  }

  #[test]
  fn interval_contains_both_not_present() {
    let interval = Interval {
      start: None,
      end: None,
    };
    assert!(interval.contains(0));
  }

  #[test]
  fn htsget_error_not_found() {
    let result = HtsGetError::not_found("error");
    assert!(matches!(result, HtsGetError::NotFound(message) if message == "error"));
  }

  #[test]
  fn htsget_error_unsupported_format() {
    let result = HtsGetError::unsupported_format("error");
    assert!(matches!(result, HtsGetError::UnsupportedFormat(message) if message == "error"));
  }

  #[test]
  fn htsget_error_invalid_input() {
    let result = HtsGetError::invalid_input("error");
    assert!(matches!(result, HtsGetError::InvalidInput(message) if message == "error"));
  }

  #[test]
  fn htsget_error_invalid_range() {
    let result = HtsGetError::invalid_range("error");
    assert!(matches!(result, HtsGetError::InvalidRange(message) if message == "error"));
  }

  #[test]
  fn htsget_error_io_error() {
    let result = HtsGetError::io_error("error");
    assert!(matches!(result, HtsGetError::IoError(message) if message == "error"));
  }

  #[test]
  fn htsget_error_parse_error() {
    let result = HtsGetError::parse_error("error");
    assert!(matches!(result, HtsGetError::ParseError(message) if message == "error"));
  }

  #[test]
  fn htsget_error_internal_error() {
    let result = HtsGetError::internal_error("error");
    assert!(matches!(result, HtsGetError::InternalError(message) if message == "error"));
  }

  #[test]
  fn query_new() {
    let result = Query::new("NA12878", Format::Bam);
    assert_eq!(result.id(), "NA12878");
  }

  #[test]
  fn query_with_format() {
    let result = Query::new("NA12878", Format::Bam);
    assert_eq!(result.format(), Format::Bam);
  }

  #[test]
  fn query_with_class() {
    let result = Query::new("NA12878", Format::Bam).with_class(Class::Header);
    assert_eq!(result.class(), Class::Header);
  }

  #[test]
  fn query_with_reference_name() {
    let result = Query::new("NA12878", Format::Bam).with_reference_name("chr1");
    assert_eq!(result.reference_name(), Some("chr1"));
  }

  #[test]
  fn query_with_start() {
    let result = Query::new("NA12878", Format::Bam).with_start(0);
    assert_eq!(result.interval().start(), Some(0));
  }

  #[test]
  fn query_with_end() {
    let result = Query::new("NA12878", Format::Bam).with_end(0);
    assert_eq!(result.interval().end(), Some(0));
  }

  #[test]
  fn query_with_fields() {
    let result =
      Query::new("NA12878", Format::Bam).with_fields(Fields::List(HashSet::from_iter(vec![
        "QNAME".to_string(),
        "FLAG".to_string(),
      ])));
    assert_eq!(
      result.fields(),
      &Fields::List(HashSet::from_iter(vec![
        "QNAME".to_string(),
        "FLAG".to_string()
      ]))
    );
  }

  #[test]
  fn query_with_tags() {
    let result = Query::new("NA12878", Format::Bam).with_tags(Tags::Tagged(TaggedTypeAll::All));
    assert_eq!(result.tags(), &Tags::Tagged(TaggedTypeAll::All));
  }

  #[test]
  fn query_with_no_tags() {
    let result = Query::new("NA12878", Format::Bam).with_no_tags(vec!["RG", "OQ"]);
    assert_eq!(
      result.no_tags(),
      &NoTags(Some(HashSet::from_iter(vec![
        "RG".to_string(),
        "OQ".to_string()
      ])))
    );
  }

  #[test]
  fn format_from_bam() {
    let result = String::from(Format::Bam);
    assert_eq!(result, "BAM");
  }

  #[test]
  fn format_from_cram() {
    let result = String::from(Format::Cram);
    assert_eq!(result, "CRAM");
  }

  #[test]
  fn format_from_vcf() {
    let result = String::from(Format::Vcf);
    assert_eq!(result, "VCF");
  }

  #[test]
  fn format_from_bcf() {
    let result = String::from(Format::Bcf);
    assert_eq!(result, "BCF");
  }

  #[test]
  fn headers_with_header() {
    let header = Headers::new(HashMap::new()).with_header("Range", "bytes=0-1023");
    let result = header.0.get("Range");
    assert_eq!(result, Some(&"bytes=0-1023".to_string()));
  }

  #[test]
  fn headers_is_empty() {
    assert!(Headers::new(HashMap::new()).is_empty());
  }

  #[test]
  fn headers_insert() {
    let mut header = Headers::new(HashMap::new());
    header.insert("Range", "bytes=0-1023");
    let result = header.0.get("Range");
    assert_eq!(result, Some(&"bytes=0-1023".to_string()));
  }

  #[test]
  fn url_with_headers() {
    let result = Url::new("data:application/vnd.ga4gh.bam;base64,QkFNAQ==")
      .with_headers(Headers::new(HashMap::new()));
    assert_eq!(result.headers, None);
  }

  #[test]
  fn url_with_class() {
    let result =
      Url::new("data:application/vnd.ga4gh.bam;base64,QkFNAQ==").with_class(Class::Header);
    assert_eq!(result.class, Some(Class::Header));
  }

  #[test]
  fn url_set_class() {
    let result =
      Url::new("data:application/vnd.ga4gh.bam;base64,QkFNAQ==").set_class(Some(Class::Header));
    assert_eq!(result.class, Some(Class::Header));
  }

  #[test]
  fn url_new() {
    let result = Url::new("data:application/vnd.ga4gh.bam;base64,QkFNAQ==");
    assert_eq!(result.url, "data:application/vnd.ga4gh.bam;base64,QkFNAQ==");
    assert_eq!(result.headers, None);
    assert_eq!(result.class, None);
  }

  #[test]
  fn response_new() {
    let result = Response::new(
      Format::Bam,
      vec![Url::new("data:application/vnd.ga4gh.bam;base64,QkFNAQ==")],
    );
    assert_eq!(result.format, Format::Bam);
    assert_eq!(
      result.urls,
      vec![Url::new("data:application/vnd.ga4gh.bam;base64,QkFNAQ==")]
    );
  }
}
