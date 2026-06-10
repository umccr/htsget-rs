use crate::error::{Result, StorageError};
use htsget_config::types::{Class, Headers, Url};
use http::HeaderMap;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;
use std::fmt::{Display, Formatter};
use tracing::instrument;

/// A DataBlock is either a range of bytes, or a data blob that gets transformed into a data uri.
#[derive(Debug, PartialEq, Eq)]
pub enum DataBlock {
  Range(BytesPosition),
  Data(Vec<u8>, Option<Class>),
}

impl DataBlock {
  /// Convert a vec of bytes positions to a vec of data blocks. Merges bytes positions.
  pub fn from_bytes_positions(positions: Vec<BytesPosition>) -> Vec<Self> {
    BytesPosition::merge_all(positions)
      .into_iter()
      .map(DataBlock::Range)
      .collect()
  }

  /// Check whether this block is empty.
  pub fn is_empty(&self) -> bool {
    match self {
      DataBlock::Range(range) => range.is_empty(),
      DataBlock::Data(data, _) => data.is_empty(),
    }
  }

  /// Update the classes of all blocks so that they all contain a class, or None. Does not merge
  /// byte positions.
  pub fn update_classes(blocks: Vec<Self>) -> Vec<Self> {
    if blocks.iter().all(|block| match block {
      DataBlock::Range(range) => range.class.is_some(),
      DataBlock::Data(_, class) => class.is_some(),
    }) {
      blocks
    } else {
      blocks
        .into_iter()
        .map(|block| match block {
          DataBlock::Range(range) => DataBlock::Range(range.set_class(None)),
          DataBlock::Data(data, _) => DataBlock::Data(data, None),
        })
        .collect()
    }
  }
}

/// A byte position has an inclusive start value, and an exclusive end value. This is analogous to
/// query start and end parameters. The class represents the class type for this byte position when
/// formatted into url responses. The class is set to `Header` for byte positions containing only
/// header bytes, `Body` for byte positions containing only body bytes, and None for byte positions
/// with a mix of header and body bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BytesPosition {
  start: Option<u64>,
  end: Option<u64>,
  class: Option<Class>,
}

/// A bytes range has an inclusive start and end value. This is analogous to http bytes ranges.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BytesRange {
  start: Option<u64>,
  end: Option<u64>,
}

impl From<&BytesRange> for String {
  fn from(ranges: &BytesRange) -> Self {
    if ranges.start.is_none() && ranges.end.is_none() {
      return "".to_string();
    }
    ranges.to_string()
  }
}

impl Display for BytesRange {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    match (self.start, self.end) {
      (Some(start), Some(end)) => write!(f, "bytes={start}-{end}"),
      (Some(0), None) | (None, None) => write!(f, ""),
      (Some(start), None) => write!(f, "bytes={start}-"),
      (None, Some(end)) => write!(f, "bytes=0-{end}"),
    }
  }
}

impl TryFrom<&BytesPosition> for BytesRange {
  type Error = StorageError;

  fn try_from(pos: &BytesPosition) -> Result<Self> {
    if pos.is_empty() {
      return Err(StorageError::InternalError(format!(
        "cannot convert a bytes position with no bytes to a bytes range: {pos:?}"
      )));
    }

    Ok(Self::new(pos.start, pos.end.map(|value| value - 1)))
  }
}

impl BytesRange {
  pub fn new(start: Option<u64>, end: Option<u64>) -> Self {
    Self { start, end }
  }
}

/// A builder for [BytesPosition].
#[derive(Clone, Debug, Default)]
pub struct BytesPositionBuilder {
  start: Option<u64>,
  end: Option<u64>,
  class: Option<Class>,
}

impl BytesPositionBuilder {
  pub fn with_start(mut self, start: u64) -> Self {
    self.start = Some(start);
    self
  }

  pub fn set_start(mut self, start: Option<u64>) -> Self {
    self.start = start;
    self
  }

  pub fn with_end(mut self, end: u64) -> Self {
    self.end = Some(end);
    self
  }

  pub fn set_end(mut self, end: Option<u64>) -> Self {
    self.end = end;
    self
  }

  pub fn with_class(mut self, class: Class) -> Self {
    self.class = Some(class);
    self
  }

  pub fn set_class(mut self, class: Option<Class>) -> Self {
    self.class = class;
    self
  }

  /// Build the bytes position, returning an error if the end is less than the start.
  pub fn build(self) -> Result<BytesPosition> {
    if self
      .end
      .is_some_and(|end| end < self.start.unwrap_or_default())
    {
      return Err(StorageError::InternalError(format!(
        "invalid bytes position, end `{:?}` is less than start `{:?}`",
        self.end, self.start
      )));
    }

    Ok(BytesPosition {
      start: self.start,
      end: self.end,
      class: self.class,
    })
  }
}

impl BytesPosition {
  /// Create a builder for a bytes position.
  pub fn builder() -> BytesPositionBuilder {
    BytesPositionBuilder::default()
  }

  pub fn with_class(self, class: Class) -> Self {
    self.set_class(Some(class))
  }

  pub fn set_class(mut self, class: Option<Class>) -> Self {
    self.class = class;
    self
  }

  pub fn get_start(&self) -> Option<u64> {
    self.start
  }

  pub fn get_end(&self) -> Option<u64> {
    self.end
  }

  pub fn get_class(&self) -> Option<Class> {
    self.class
  }

  /// Check whether this position selects no bytes, i.e. the end == start.
  pub fn is_empty(&self) -> bool {
    self
      .end
      .is_some_and(|end| end == self.start.unwrap_or_default())
  }

  pub fn overlaps(&self, range: &BytesPosition) -> bool {
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

  /// Merges position with the current BytesPosition, assuming that the two positions overlap.
  pub fn merge_with(&mut self, position: &BytesPosition) -> &Self {
    let start = self.start;
    let end = self.end;

    self.start = match (start.as_ref(), position.start.as_ref()) {
      (None, None) | (None, Some(_)) | (Some(_), None) => None,
      (Some(a), Some(b)) => Some(*a.min(b)),
    };
    self.end = match (end.as_ref(), position.end.as_ref()) {
      (None, None) | (None, Some(_)) | (Some(_), None) => None,
      (Some(a), Some(b)) => Some(*a.max(b)),
    };

    self.class = match (self.class.as_ref(), position.class.as_ref()) {
      (Some(Class::Header), Some(Class::Header)) => Some(Class::Header),
      (Some(Class::Body), Some(Class::Body)) => Some(Class::Body),
      (_, _) => None,
    };

    self
  }

  /// Merge ranges, assuming ending byte ranges are exclusive.
  #[instrument(level = "trace", ret)]
  pub fn merge_all(mut ranges: Vec<BytesPosition>) -> Vec<BytesPosition> {
    ranges.retain(|range| !range.is_empty());

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
          current_range.merge_with(range);
        } else {
          optimized_ranges.push(current_range);
          current_range = range.clone();
        }
      }

      optimized_ranges.push(current_range);

      optimized_ranges
    }
  }
}

#[derive(Debug, Clone)]
pub struct GetOptions<'a> {
  pub(crate) range: BytesPosition,
  pub(crate) request_headers: Cow<'a, HeaderMap>,
}

impl<'a> GetOptions<'a> {
  pub fn new(range: BytesPosition, request_headers: &'a HeaderMap) -> Self {
    Self {
      range,
      request_headers: Cow::Borrowed(request_headers),
    }
  }

  pub fn new_with_default_range(request_headers: &'a HeaderMap) -> Self {
    Self::new(Default::default(), request_headers)
  }

  pub fn with_max_length(mut self, max_length: u64) -> Result<Self> {
    self.range = BytesPosition::builder()
      .with_start(0)
      .with_end(max_length)
      .build()?;
    Ok(self)
  }

  pub fn with_range(mut self, range: BytesPosition) -> Self {
    self.range = range;
    self
  }

  /// Get the range.
  pub fn range(&self) -> &BytesPosition {
    &self.range
  }

  /// Get the request headers.
  pub fn request_headers(&self) -> &HeaderMap {
    self.request_headers.as_ref()
  }

  /// Set the request headers from an owned value.
  pub fn set_request_headers(&mut self, request_headers: HeaderMap) {
    self.request_headers = Cow::Owned(request_headers);
  }
}

#[derive(Debug, Clone)]
pub struct BytesPositionOptions<'a> {
  pub(crate) positions: Vec<BytesPosition>,
  pub(crate) headers: &'a HeaderMap,
}

impl<'a> BytesPositionOptions<'a> {
  pub fn new(positions: Vec<BytesPosition>, headers: &'a HeaderMap) -> Self {
    Self { positions, headers }
  }

  /// Get the response headers.
  pub fn headers(&self) -> &'a HeaderMap {
    self.headers
  }

  pub fn positions(&self) -> &Vec<BytesPosition> {
    &self.positions
  }

  /// Get the inner value.
  pub fn into_inner(self) -> Vec<BytesPosition> {
    self.positions
  }

  /// Merge all bytes positions
  pub fn merge_all(mut self) -> Self {
    self.positions = BytesPosition::merge_all(self.positions);
    self
  }
}

#[derive(Debug, Clone)]
pub struct RangeUrlOptions<'a> {
  range: BytesPosition,
  response_headers: &'a HeaderMap,
}

impl<'a> RangeUrlOptions<'a> {
  pub fn new(range: BytesPosition, response_headers: &'a HeaderMap) -> Self {
    Self {
      range,
      response_headers,
    }
  }

  pub fn new_with_default_range(request_headers: &'a HeaderMap) -> Self {
    Self::new(Default::default(), request_headers)
  }

  pub fn with_range(mut self, range: BytesPosition) -> Self {
    self.range = range;
    self
  }

  pub fn apply(self, url: Url) -> Result<Url> {
    let range: String = String::from(&BytesRange::try_from(self.range())?);

    let url = if range.is_empty() {
      url
    } else {
      url.add_headers(Headers::default().with_header("Range", range))
    };

    Ok(url.set_class(self.range().class))
  }

  /// Get the range.
  pub fn range(&self) -> &BytesPosition {
    &self.range
  }

  /// Get the response headers.
  pub fn response_headers(&self) -> &'a HeaderMap {
    self.response_headers
  }
}

/// A struct to represent options passed to a `Storage` head call.
#[derive(Debug, Clone)]
pub struct HeadOptions<'a> {
  request_headers: &'a HeaderMap,
}

impl<'a> HeadOptions<'a> {
  /// Create a new HeadOptions struct.
  pub fn new(request_headers: &'a HeaderMap) -> Self {
    Self { request_headers }
  }

  /// Get the request headers.
  pub fn request_headers(&self) -> &'a HeaderMap {
    self.request_headers
  }
}

impl<'a> From<&'a GetOptions<'a>> for HeadOptions<'a> {
  fn from(options: &'a GetOptions<'a>) -> Self {
    Self::new(options.request_headers())
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use super::*;

  #[test]
  fn bytes_range_overlapping_and_merge() {
    let test_cases = vec![
      (
        BytesPosition::builder().with_end(2).build().unwrap(),
        BytesPosition::builder()
          .with_start(3)
          .with_end(5)
          .build()
          .unwrap(),
        None,
      ),
      (
        BytesPosition::builder().with_end(2).build().unwrap(),
        BytesPosition::builder().with_start(3).build().unwrap(),
        None,
      ),
      (
        BytesPosition::builder().with_end(2).build().unwrap(),
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        Some(BytesPosition::builder().with_end(4).build().unwrap()),
      ),
      (
        BytesPosition::builder().with_end(2).build().unwrap(),
        BytesPosition::builder().with_start(2).build().unwrap(),
        Some(BytesPosition::builder().build().unwrap()),
      ),
      (
        BytesPosition::builder().with_end(2).build().unwrap(),
        BytesPosition::builder()
          .with_start(1)
          .with_end(3)
          .build()
          .unwrap(),
        Some(BytesPosition::builder().with_end(3).build().unwrap()),
      ),
      (
        BytesPosition::builder().with_end(2).build().unwrap(),
        BytesPosition::builder().with_start(1).build().unwrap(),
        Some(BytesPosition::builder().build().unwrap()),
      ),
      (
        BytesPosition::builder().with_end(2).build().unwrap(),
        BytesPosition::builder()
          .with_start(0)
          .with_end(2)
          .build()
          .unwrap(),
        Some(BytesPosition::builder().with_end(2).build().unwrap()),
      ),
      (
        BytesPosition::builder().with_end(2).build().unwrap(),
        BytesPosition::builder().with_end(2).build().unwrap(),
        Some(BytesPosition::builder().with_end(2).build().unwrap()),
      ),
      (
        BytesPosition::builder().with_end(2).build().unwrap(),
        BytesPosition::builder()
          .with_start(0)
          .with_end(1)
          .build()
          .unwrap(),
        Some(BytesPosition::builder().with_end(2).build().unwrap()),
      ),
      (
        BytesPosition::builder().with_end(2).build().unwrap(),
        BytesPosition::builder().with_end(1).build().unwrap(),
        Some(BytesPosition::builder().with_end(2).build().unwrap()),
      ),
      (
        BytesPosition::builder().with_end(2).build().unwrap(),
        BytesPosition::builder().build().unwrap(),
        Some(BytesPosition::builder().build().unwrap()),
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_start(6)
          .with_end(8)
          .build()
          .unwrap(),
        None,
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder().with_start(6).build().unwrap(),
        None,
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_start(4)
          .with_end(6)
          .build()
          .unwrap(),
        Some(
          BytesPosition::builder()
            .with_start(2)
            .with_end(6)
            .build()
            .unwrap(),
        ),
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder().with_start(4).build().unwrap(),
        Some(BytesPosition::builder().with_start(2).build().unwrap()),
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_start(3)
          .with_end(5)
          .build()
          .unwrap(),
        Some(
          BytesPosition::builder()
            .with_start(2)
            .with_end(5)
            .build()
            .unwrap(),
        ),
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder().with_start(3).build().unwrap(),
        Some(BytesPosition::builder().with_start(2).build().unwrap()),
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_start(2)
          .with_end(3)
          .build()
          .unwrap(),
        Some(
          BytesPosition::builder()
            .with_start(2)
            .with_end(4)
            .build()
            .unwrap(),
        ),
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder().with_end(3).build().unwrap(),
        Some(BytesPosition::builder().with_end(4).build().unwrap()),
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_start(1)
          .with_end(3)
          .build()
          .unwrap(),
        Some(
          BytesPosition::builder()
            .with_start(1)
            .with_end(4)
            .build()
            .unwrap(),
        ),
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder().with_end(3).build().unwrap(),
        Some(BytesPosition::builder().with_end(4).build().unwrap()),
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_start(0)
          .with_end(2)
          .build()
          .unwrap(),
        Some(
          BytesPosition::builder()
            .with_start(0)
            .with_end(4)
            .build()
            .unwrap(),
        ),
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder().with_end(2).build().unwrap(),
        Some(BytesPosition::builder().with_end(4).build().unwrap()),
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_start(0)
          .with_end(1)
          .build()
          .unwrap(),
        None,
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder().with_end(1).build().unwrap(),
        None,
      ),
      (
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        BytesPosition::builder().build().unwrap(),
        Some(BytesPosition::builder().build().unwrap()),
      ),
      (
        BytesPosition::builder().with_start(2).build().unwrap(),
        BytesPosition::builder()
          .with_start(4)
          .with_end(6)
          .build()
          .unwrap(),
        Some(BytesPosition::builder().with_start(2).build().unwrap()),
      ),
      (
        BytesPosition::builder().with_start(2).build().unwrap(),
        BytesPosition::builder().with_start(4).build().unwrap(),
        Some(BytesPosition::builder().with_start(2).build().unwrap()),
      ),
      (
        BytesPosition::builder().with_start(2).build().unwrap(),
        BytesPosition::builder()
          .with_start(2)
          .with_end(4)
          .build()
          .unwrap(),
        Some(BytesPosition::builder().with_start(2).build().unwrap()),
      ),
      (
        BytesPosition::builder().with_start(2).build().unwrap(),
        BytesPosition::builder().with_start(2).build().unwrap(),
        Some(BytesPosition::builder().with_start(2).build().unwrap()),
      ),
      (
        BytesPosition::builder().with_start(2).build().unwrap(),
        BytesPosition::builder()
          .with_start(1)
          .with_end(3)
          .build()
          .unwrap(),
        Some(BytesPosition::builder().with_start(1).build().unwrap()),
      ),
      (
        BytesPosition::builder().with_start(2).build().unwrap(),
        BytesPosition::builder().with_end(3).build().unwrap(),
        Some(BytesPosition::builder().build().unwrap()),
      ),
      (
        BytesPosition::builder().with_start(2).build().unwrap(),
        BytesPosition::builder()
          .with_start(0)
          .with_end(2)
          .build()
          .unwrap(),
        Some(BytesPosition::builder().with_start(0).build().unwrap()),
      ),
      (
        BytesPosition::builder().with_start(2).build().unwrap(),
        BytesPosition::builder().with_end(2).build().unwrap(),
        Some(BytesPosition::builder().build().unwrap()),
      ),
      (
        BytesPosition::builder().with_start(2).build().unwrap(),
        BytesPosition::builder()
          .with_start(0)
          .with_end(1)
          .build()
          .unwrap(),
        None,
      ),
      (
        BytesPosition::builder().with_start(2).build().unwrap(),
        BytesPosition::builder().with_end(1).build().unwrap(),
        None,
      ),
      (
        BytesPosition::builder().with_start(2).build().unwrap(),
        BytesPosition::builder().build().unwrap(),
        Some(BytesPosition::builder().build().unwrap()),
      ),
      (
        BytesPosition::builder().build().unwrap(),
        BytesPosition::builder().build().unwrap(),
        Some(BytesPosition::builder().build().unwrap()),
      ),
    ];

    for (index, (a, b, expected)) in test_cases.iter().enumerate() {
      println!("Test case {index}");
      println!("  {a:?}");
      println!("  {b:?}");
      println!("  {expected:?}");

      if a.overlaps(b) {
        assert_eq!(*a.clone().merge_with(b), expected.clone().unwrap());
      } else {
        assert!(expected.is_none())
      }
    }
  }

  #[test]
  fn bytes_range_merge_all_when_list_is_empty() {
    assert_eq!(BytesPosition::merge_all(Vec::new()), Vec::new());
  }

  #[test]
  fn bytes_range_merge_all_removes_empty_positions() {
    assert_eq!(
      BytesPosition::merge_all(vec![
        BytesPosition::builder()
          .with_start(0)
          .with_end(0)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_start(5)
          .with_end(5)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_start(1)
          .with_end(2)
          .build()
          .unwrap(),
      ]),
      vec![
        BytesPosition::builder()
          .with_start(1)
          .with_end(2)
          .build()
          .unwrap()
      ]
    );
  }

  #[test]
  fn bytes_position_is_empty() {
    assert!(
      BytesPosition::builder()
        .with_start(0)
        .with_end(0)
        .build()
        .unwrap()
        .is_empty()
    );
    assert!(
      BytesPosition::builder()
        .with_end(0)
        .build()
        .unwrap()
        .is_empty()
    );
    assert!(
      !BytesPosition::builder()
        .with_start(0)
        .with_end(1)
        .build()
        .unwrap()
        .is_empty()
    );
    assert!(
      !BytesPosition::builder()
        .with_start(1)
        .build()
        .unwrap()
        .is_empty()
    );
    assert!(!BytesPosition::builder().build().unwrap().is_empty());
  }

  #[test]
  fn bytes_position_end_less_than_start() {
    assert!(
      BytesPosition::builder()
        .with_start(2)
        .with_end(1)
        .build()
        .is_err()
    );
    assert!(BytesPosition::builder().with_end(1).build().is_ok());
    assert!(
      BytesPosition::builder()
        .with_end(2)
        .with_start(3)
        .build()
        .is_err()
    );
    assert!(
      BytesPosition::builder()
        .with_start(3)
        .with_end(2)
        .build()
        .is_err()
    );
    assert!(
      BytesPosition::builder()
        .with_start(3)
        .set_end(Some(2))
        .build()
        .is_err()
    );
  }

  #[test]
  fn bytes_position_builder() {
    let result = BytesPosition::builder()
      .with_end(2)
      .with_start(5)
      .with_end(10)
      .build();
    assert_eq!(
      result.unwrap(),
      BytesPosition::builder()
        .with_start(5)
        .with_end(10)
        .build()
        .unwrap()
    );
  }

  #[test]
  fn bytes_range_try_from_empty_position() {
    assert!(
      BytesRange::try_from(
        &BytesPosition::builder()
          .with_start(5)
          .with_end(5)
          .build()
          .unwrap()
      )
      .is_err()
    );
    assert!(BytesRange::try_from(&BytesPosition::builder().with_end(0).build().unwrap()).is_err());
  }

  #[test]
  fn bytes_range_merge_all_when_list_has_one_range() {
    assert_eq!(
      BytesPosition::merge_all(vec![BytesPosition::default()]),
      vec![BytesPosition::default()]
    );
  }

  #[test]
  fn bytes_position_merge_class_header() {
    assert_eq!(
      BytesPosition::merge_all(vec![
        BytesPosition::builder()
          .with_end(1)
          .with_class(Class::Header)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_end(2)
          .with_class(Class::Header)
          .build()
          .unwrap()
      ]),
      vec![
        BytesPosition::builder()
          .with_end(2)
          .with_class(Class::Header)
          .build()
          .unwrap()
      ]
    );
  }

  #[test]
  fn bytes_position_merge_class_body() {
    assert_eq!(
      BytesPosition::merge_all(vec![
        BytesPosition::builder()
          .with_end(1)
          .with_class(Class::Body)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_end(3)
          .with_class(Class::Body)
          .build()
          .unwrap()
      ]),
      vec![
        BytesPosition::builder()
          .with_end(3)
          .with_class(Class::Body)
          .build()
          .unwrap()
      ]
    );
  }

  #[test]
  fn bytes_position_merge_class_none() {
    assert_eq!(
      BytesPosition::merge_all(vec![
        BytesPosition::builder()
          .with_start(1)
          .with_end(2)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_start(2)
          .with_end(3)
          .build()
          .unwrap()
      ]),
      vec![
        BytesPosition::builder()
          .with_start(1)
          .with_end(3)
          .build()
          .unwrap()
      ]
    );
  }

  #[test]
  fn bytes_position_merge_class_different() {
    assert_eq!(
      BytesPosition::merge_all(vec![
        BytesPosition::builder()
          .with_start(1)
          .with_end(2)
          .with_class(Class::Header)
          .build()
          .unwrap(),
        BytesPosition::builder()
          .with_start(2)
          .with_end(3)
          .with_class(Class::Body)
          .build()
          .unwrap()
      ]),
      vec![
        BytesPosition::builder()
          .with_start(1)
          .with_end(3)
          .build()
          .unwrap()
      ]
    );
  }

  #[test]
  fn bytes_range_merge_all_when_list_has_many_ranges() {
    let ranges = vec![
      BytesPosition::builder().with_end(1).build().unwrap(),
      BytesPosition::builder()
        .with_start(1)
        .with_end(2)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(5)
        .with_end(6)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(5)
        .with_end(8)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(6)
        .with_end(7)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(4)
        .with_end(5)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(3)
        .with_end(6)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(10)
        .with_end(12)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(10)
        .with_end(12)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(10)
        .with_end(14)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(14)
        .with_end(15)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(12)
        .with_end(16)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(17)
        .with_end(19)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(21)
        .with_end(23)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(18)
        .with_end(22)
        .build()
        .unwrap(),
      BytesPosition::builder().with_start(24).build().unwrap(),
      BytesPosition::builder()
        .with_start(24)
        .with_end(30)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(31)
        .with_end(33)
        .build()
        .unwrap(),
      BytesPosition::builder().with_start(35).build().unwrap(),
    ];

    let expected_ranges = vec![
      BytesPosition::builder().with_end(2).build().unwrap(),
      BytesPosition::builder()
        .with_start(3)
        .with_end(8)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(10)
        .with_end(16)
        .build()
        .unwrap(),
      BytesPosition::builder()
        .with_start(17)
        .with_end(23)
        .build()
        .unwrap(),
      BytesPosition::builder().with_start(24).build().unwrap(),
    ];

    assert_eq!(BytesPosition::merge_all(ranges), expected_ranges);
  }

  #[test]
  fn bytes_position_new() {
    let result = BytesPosition::builder()
      .with_start(1)
      .with_end(2)
      .with_class(Class::Header)
      .build()
      .unwrap();
    assert_eq!(result.start, Some(1));
    assert_eq!(result.end, Some(2));
    assert_eq!(result.class, Some(Class::Header));
  }

  #[test]
  fn bytes_position_with_start() {
    let result = BytesPosition::builder().with_start(1).build().unwrap();
    assert_eq!(result.start, Some(1));
  }

  #[test]
  fn bytes_position_with_end() {
    let result = BytesPosition::builder().with_end(1).build().unwrap();
    assert_eq!(result.end, Some(1));
  }

  #[test]
  fn bytes_position_with_class() {
    let result = BytesPosition::default().with_class(Class::Header);
    assert_eq!(result.class, Some(Class::Header));
  }

  #[test]
  fn bytes_position_set_class() {
    let result = BytesPosition::default().set_class(Some(Class::Header));
    assert_eq!(result.class, Some(Class::Header));
  }

  #[test]
  fn data_block_update_classes_all_some() {
    let blocks = DataBlock::update_classes(vec![
      DataBlock::Range(
        BytesPosition::builder()
          .with_end(1)
          .with_class(Class::Body)
          .build()
          .unwrap(),
      ),
      DataBlock::Data(vec![], Some(Class::Header)),
    ]);
    for block in blocks {
      let class = match block {
        DataBlock::Range(pos) => pos.class,
        DataBlock::Data(_, class) => class,
      };
      assert!(class.is_some());
    }
  }

  #[test]
  fn data_block_update_classes_one_none() {
    let blocks = DataBlock::update_classes(vec![
      DataBlock::Range(
        BytesPosition::builder()
          .with_end(1)
          .with_class(Class::Body)
          .build()
          .unwrap(),
      ),
      DataBlock::Data(vec![], None),
    ]);
    for block in blocks {
      let class = match block {
        DataBlock::Range(pos) => pos.class,
        DataBlock::Data(_, class) => class,
      };
      assert!(class.is_none());
    }
  }

  #[test]
  fn data_block_from_bytes_positions() {
    let blocks = DataBlock::from_bytes_positions(vec![
      BytesPosition::builder().with_end(1).build().unwrap(),
      BytesPosition::builder()
        .with_start(1)
        .with_end(2)
        .build()
        .unwrap(),
    ]);
    assert_eq!(
      blocks,
      vec![DataBlock::Range(
        BytesPosition::builder().with_end(2).build().unwrap()
      )]
    );
  }

  #[test]
  fn data_block_from_empty_bytes_positions() {
    let blocks = DataBlock::from_bytes_positions(vec![
      BytesPosition::builder()
        .with_start(0)
        .with_end(0)
        .build()
        .unwrap(),
    ]);
    assert_eq!(blocks, vec![]);
  }

  #[test]
  fn data_block_is_empty() {
    assert!(
      DataBlock::Range(
        BytesPosition::builder()
          .with_start(0)
          .with_end(0)
          .build()
          .unwrap()
      )
      .is_empty()
    );
    assert!(DataBlock::Data(vec![], None).is_empty());
    assert!(
      !DataBlock::Range(
        BytesPosition::builder()
          .with_start(0)
          .with_end(1)
          .build()
          .unwrap()
      )
      .is_empty()
    );
    assert!(!DataBlock::Data(vec![0], None).is_empty());
  }

  #[test]
  fn byte_range_from_byte_position() {
    let result = BytesRange::try_from(
      &BytesPosition::builder()
        .with_start(5)
        .with_end(10)
        .build()
        .unwrap(),
    )
    .unwrap();
    let expected = BytesRange::new(Some(5), Some(9));
    assert_eq!(result, expected);
  }

  #[test]
  fn get_options_with_max_length() {
    let request_headers = Default::default();
    let result = GetOptions::new_with_default_range(&request_headers)
      .with_max_length(1)
      .unwrap();
    assert_eq!(
      result.range(),
      &BytesPosition::builder()
        .with_start(0)
        .with_end(1)
        .build()
        .unwrap()
    );
  }

  #[test]
  fn get_options_with_range() {
    let request_headers = Default::default();
    let result = GetOptions::new_with_default_range(&request_headers).with_range(
      BytesPosition::builder()
        .with_start(5)
        .with_end(11)
        .with_class(Class::Header)
        .build()
        .unwrap(),
    );
    assert_eq!(
      result.range(),
      &BytesPosition::builder()
        .with_start(5)
        .with_end(11)
        .with_class(Class::Header)
        .build()
        .unwrap()
    );
  }

  #[test]
  fn url_options_with_range() {
    let request_headers = Default::default();
    let result = RangeUrlOptions::new_with_default_range(&request_headers).with_range(
      BytesPosition::builder()
        .with_start(5)
        .with_end(11)
        .with_class(Class::Header)
        .build()
        .unwrap(),
    );
    assert_eq!(
      result.range(),
      &BytesPosition::builder()
        .with_start(5)
        .with_end(11)
        .with_class(Class::Header)
        .build()
        .unwrap()
    );
  }

  #[test]
  fn url_options_apply_with_bytes_range() {
    let result = RangeUrlOptions::new(
      BytesPosition::builder()
        .with_start(5)
        .with_end(11)
        .with_class(Class::Header)
        .build()
        .unwrap(),
      &Default::default(),
    )
    .apply(Url::new(""))
    .unwrap();
    println!("{result:?}");
    assert_eq!(
      result,
      Url::new("")
        .with_headers(Headers::new(HashMap::new()).with_header("Range", "bytes=5-10"))
        .with_class(Class::Header)
    );
  }

  #[test]
  fn url_options_apply_no_bytes_range() {
    let result = RangeUrlOptions::new_with_default_range(&Default::default())
      .apply(Url::new(""))
      .unwrap();
    assert_eq!(result, Url::new(""));
  }

  #[test]
  fn url_options_apply_with_headers() {
    let result = RangeUrlOptions::new(
      BytesPosition::builder()
        .with_start(5)
        .with_end(11)
        .with_class(Class::Header)
        .build()
        .unwrap(),
      &Default::default(),
    )
    .apply(Url::new("").with_headers(Headers::default().with_header("header", "value")))
    .unwrap();
    println!("{result:?}");

    assert_eq!(
      result,
      Url::new("")
        .with_headers(
          Headers::new(HashMap::new())
            .with_header("Range", "bytes=5-10")
            .with_header("header", "value")
        )
        .with_class(Class::Header)
    );
  }
}
