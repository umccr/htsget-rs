extern crate core;

use noodles::core::region::Interval as NoodlesInterval;
use noodles::core::Position;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Formatter;
use std::io::ErrorKind::Other;
use std::{fmt, io};
use tracing::instrument;

pub mod config;
pub mod resolver;
pub mod storage;

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

  pub fn with_id(mut self, id: impl Into<String>) -> Self {
    self.id = id.into();
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

#[cfg(test)]
mod tests {
  use crate::Interval;

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
}
