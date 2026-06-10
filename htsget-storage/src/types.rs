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

impl BytesPosition {
  /// Create a new bytes position. Returns an error if the end is less than the start.
  /// An empty `BytesPosition` is allowed by setting the start and end to the same value.
  pub fn new(start: Option<u64>, end: Option<u64>, class: Option<Class>) -> Result<Self> {
    Self { start, end, class }.validate()
  }

  /// Validate that the end is greater than the start.
  fn validate(self) -> Result<Self> {
    if self
      .end
      .is_some_and(|end| end < self.start.unwrap_or_default())
    {
      return Err(StorageError::InternalError(format!(
        "invalid bytes position {self:?}"
      )));
    }

    Ok(self)
  }

  pub fn with_start(mut self, start: u64) -> Result<Self> {
    self.start = Some(start);
    self.validate()
  }

  pub fn with_end(self, end: u64) -> Result<Self> {
    self.set_end(Some(end))
  }

  pub fn set_end(mut self, end: Option<u64>) -> Result<Self> {
    self.end = end;
    self.validate()
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
    self.range = BytesPosition::default()
      .with_start(0)?
      .with_end(max_length)?;
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
        BytesPosition::new(None, Some(2), None).unwrap(),
        BytesPosition::new(Some(3), Some(5), None).unwrap(),
        None,
      ),
      (
        BytesPosition::new(None, Some(2), None).unwrap(),
        BytesPosition::new(Some(3), None, None).unwrap(),
        None,
      ),
      (
        BytesPosition::new(None, Some(2), None).unwrap(),
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        Some(BytesPosition::new(None, Some(4), None).unwrap()),
      ),
      (
        BytesPosition::new(None, Some(2), None).unwrap(),
        BytesPosition::new(Some(2), None, None).unwrap(),
        Some(BytesPosition::new(None, None, None).unwrap()),
      ),
      (
        BytesPosition::new(None, Some(2), None).unwrap(),
        BytesPosition::new(Some(1), Some(3), None).unwrap(),
        Some(BytesPosition::new(None, Some(3), None).unwrap()),
      ),
      (
        BytesPosition::new(None, Some(2), None).unwrap(),
        BytesPosition::new(Some(1), None, None).unwrap(),
        Some(BytesPosition::new(None, None, None).unwrap()),
      ),
      (
        BytesPosition::new(None, Some(2), None).unwrap(),
        BytesPosition::new(Some(0), Some(2), None).unwrap(),
        Some(BytesPosition::new(None, Some(2), None).unwrap()),
      ),
      (
        BytesPosition::new(None, Some(2), None).unwrap(),
        BytesPosition::new(None, Some(2), None).unwrap(),
        Some(BytesPosition::new(None, Some(2), None).unwrap()),
      ),
      (
        BytesPosition::new(None, Some(2), None).unwrap(),
        BytesPosition::new(Some(0), Some(1), None).unwrap(),
        Some(BytesPosition::new(None, Some(2), None).unwrap()),
      ),
      (
        BytesPosition::new(None, Some(2), None).unwrap(),
        BytesPosition::new(None, Some(1), None).unwrap(),
        Some(BytesPosition::new(None, Some(2), None).unwrap()),
      ),
      (
        BytesPosition::new(None, Some(2), None).unwrap(),
        BytesPosition::new(None, None, None).unwrap(),
        Some(BytesPosition::new(None, None, None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(Some(6), Some(8), None).unwrap(),
        None,
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(Some(6), None, None).unwrap(),
        None,
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(Some(4), Some(6), None).unwrap(),
        Some(BytesPosition::new(Some(2), Some(6), None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(Some(4), None, None).unwrap(),
        Some(BytesPosition::new(Some(2), None, None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(Some(3), Some(5), None).unwrap(),
        Some(BytesPosition::new(Some(2), Some(5), None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(Some(3), None, None).unwrap(),
        Some(BytesPosition::new(Some(2), None, None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(Some(2), Some(3), None).unwrap(),
        Some(BytesPosition::new(Some(2), Some(4), None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(None, Some(3), None).unwrap(),
        Some(BytesPosition::new(None, Some(4), None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(Some(1), Some(3), None).unwrap(),
        Some(BytesPosition::new(Some(1), Some(4), None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(None, Some(3), None).unwrap(),
        Some(BytesPosition::new(None, Some(4), None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(Some(0), Some(2), None).unwrap(),
        Some(BytesPosition::new(Some(0), Some(4), None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(None, Some(2), None).unwrap(),
        Some(BytesPosition::new(None, Some(4), None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(Some(0), Some(1), None).unwrap(),
        None,
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(None, Some(1), None).unwrap(),
        None,
      ),
      (
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        BytesPosition::new(None, None, None).unwrap(),
        Some(BytesPosition::new(None, None, None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), None, None).unwrap(),
        BytesPosition::new(Some(4), Some(6), None).unwrap(),
        Some(BytesPosition::new(Some(2), None, None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), None, None).unwrap(),
        BytesPosition::new(Some(4), None, None).unwrap(),
        Some(BytesPosition::new(Some(2), None, None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), None, None).unwrap(),
        BytesPosition::new(Some(2), Some(4), None).unwrap(),
        Some(BytesPosition::new(Some(2), None, None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), None, None).unwrap(),
        BytesPosition::new(Some(2), None, None).unwrap(),
        Some(BytesPosition::new(Some(2), None, None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), None, None).unwrap(),
        BytesPosition::new(Some(1), Some(3), None).unwrap(),
        Some(BytesPosition::new(Some(1), None, None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), None, None).unwrap(),
        BytesPosition::new(None, Some(3), None).unwrap(),
        Some(BytesPosition::new(None, None, None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), None, None).unwrap(),
        BytesPosition::new(Some(0), Some(2), None).unwrap(),
        Some(BytesPosition::new(Some(0), None, None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), None, None).unwrap(),
        BytesPosition::new(None, Some(2), None).unwrap(),
        Some(BytesPosition::new(None, None, None).unwrap()),
      ),
      (
        BytesPosition::new(Some(2), None, None).unwrap(),
        BytesPosition::new(Some(0), Some(1), None).unwrap(),
        None,
      ),
      (
        BytesPosition::new(Some(2), None, None).unwrap(),
        BytesPosition::new(None, Some(1), None).unwrap(),
        None,
      ),
      (
        BytesPosition::new(Some(2), None, None).unwrap(),
        BytesPosition::new(None, None, None).unwrap(),
        Some(BytesPosition::new(None, None, None).unwrap()),
      ),
      (
        BytesPosition::new(None, None, None).unwrap(),
        BytesPosition::new(None, None, None).unwrap(),
        Some(BytesPosition::new(None, None, None).unwrap()),
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
        BytesPosition::new(Some(0), Some(0), None).unwrap(),
        BytesPosition::new(Some(5), Some(5), None).unwrap(),
        BytesPosition::new(Some(1), Some(2), None).unwrap(),
      ]),
      vec![BytesPosition::new(Some(1), Some(2), None).unwrap()]
    );
  }

  #[test]
  fn bytes_position_is_empty() {
    assert!(
      BytesPosition::new(Some(0), Some(0), None)
        .unwrap()
        .is_empty()
    );
    assert!(BytesPosition::new(None, Some(0), None).unwrap().is_empty());
    assert!(
      !BytesPosition::new(Some(0), Some(1), None)
        .unwrap()
        .is_empty()
    );
    assert!(!BytesPosition::new(Some(1), None, None).unwrap().is_empty());
    assert!(!BytesPosition::new(None, None, None).unwrap().is_empty());
  }

  #[test]
  fn bytes_position_end_less_than_start() {
    assert!(BytesPosition::new(Some(2), Some(1), None).is_err());
    assert!(BytesPosition::new(None, Some(1), None).is_ok());
    assert!(
      BytesPosition::default()
        .with_end(2)
        .unwrap()
        .with_start(3)
        .is_err()
    );
    assert!(
      BytesPosition::default()
        .with_start(3)
        .unwrap()
        .with_end(2)
        .is_err()
    );
    assert!(
      BytesPosition::default()
        .with_start(3)
        .unwrap()
        .set_end(Some(2))
        .is_err()
    );
  }

  #[test]
  fn bytes_range_try_from_empty_position() {
    assert!(BytesRange::try_from(&BytesPosition::new(Some(5), Some(5), None).unwrap()).is_err());
    assert!(BytesRange::try_from(&BytesPosition::new(None, Some(0), None).unwrap()).is_err());
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
        BytesPosition::new(None, Some(1), Some(Class::Header)).unwrap(),
        BytesPosition::new(None, Some(2), Some(Class::Header)).unwrap()
      ]),
      vec![BytesPosition::new(None, Some(2), Some(Class::Header)).unwrap()]
    );
  }

  #[test]
  fn bytes_position_merge_class_body() {
    assert_eq!(
      BytesPosition::merge_all(vec![
        BytesPosition::new(None, Some(1), Some(Class::Body)).unwrap(),
        BytesPosition::new(None, Some(3), Some(Class::Body)).unwrap()
      ]),
      vec![BytesPosition::new(None, Some(3), Some(Class::Body)).unwrap()]
    );
  }

  #[test]
  fn bytes_position_merge_class_none() {
    assert_eq!(
      BytesPosition::merge_all(vec![
        BytesPosition::new(Some(1), Some(2), None).unwrap(),
        BytesPosition::new(Some(2), Some(3), None).unwrap()
      ]),
      vec![BytesPosition::new(Some(1), Some(3), None).unwrap()]
    );
  }

  #[test]
  fn bytes_position_merge_class_different() {
    assert_eq!(
      BytesPosition::merge_all(vec![
        BytesPosition::new(Some(1), Some(2), Some(Class::Header)).unwrap(),
        BytesPosition::new(Some(2), Some(3), Some(Class::Body)).unwrap()
      ]),
      vec![BytesPosition::new(Some(1), Some(3), None).unwrap()]
    );
  }

  #[test]
  fn bytes_range_merge_all_when_list_has_many_ranges() {
    let ranges = vec![
      BytesPosition::new(None, Some(1), None).unwrap(),
      BytesPosition::new(Some(1), Some(2), None).unwrap(),
      BytesPosition::new(Some(5), Some(6), None).unwrap(),
      BytesPosition::new(Some(5), Some(8), None).unwrap(),
      BytesPosition::new(Some(6), Some(7), None).unwrap(),
      BytesPosition::new(Some(4), Some(5), None).unwrap(),
      BytesPosition::new(Some(3), Some(6), None).unwrap(),
      BytesPosition::new(Some(10), Some(12), None).unwrap(),
      BytesPosition::new(Some(10), Some(12), None).unwrap(),
      BytesPosition::new(Some(10), Some(14), None).unwrap(),
      BytesPosition::new(Some(14), Some(15), None).unwrap(),
      BytesPosition::new(Some(12), Some(16), None).unwrap(),
      BytesPosition::new(Some(17), Some(19), None).unwrap(),
      BytesPosition::new(Some(21), Some(23), None).unwrap(),
      BytesPosition::new(Some(18), Some(22), None).unwrap(),
      BytesPosition::new(Some(24), None, None).unwrap(),
      BytesPosition::new(Some(24), Some(30), None).unwrap(),
      BytesPosition::new(Some(31), Some(33), None).unwrap(),
      BytesPosition::new(Some(35), None, None).unwrap(),
    ];

    let expected_ranges = vec![
      BytesPosition::new(None, Some(2), None).unwrap(),
      BytesPosition::new(Some(3), Some(8), None).unwrap(),
      BytesPosition::new(Some(10), Some(16), None).unwrap(),
      BytesPosition::new(Some(17), Some(23), None).unwrap(),
      BytesPosition::new(Some(24), None, None).unwrap(),
    ];

    assert_eq!(BytesPosition::merge_all(ranges), expected_ranges);
  }

  #[test]
  fn bytes_position_new() {
    let result = BytesPosition::new(Some(1), Some(2), Some(Class::Header)).unwrap();
    assert_eq!(result.start, Some(1));
    assert_eq!(result.end, Some(2));
    assert_eq!(result.class, Some(Class::Header));
  }

  #[test]
  fn bytes_position_with_start() {
    let result = BytesPosition::default().with_start(1).unwrap();
    assert_eq!(result.start, Some(1));
  }

  #[test]
  fn bytes_position_with_end() {
    let result = BytesPosition::default().with_end(1).unwrap();
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
      DataBlock::Range(BytesPosition::new(None, Some(1), Some(Class::Body)).unwrap()),
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
      DataBlock::Range(BytesPosition::new(None, Some(1), Some(Class::Body)).unwrap()),
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
      BytesPosition::new(None, Some(1), None).unwrap(),
      BytesPosition::new(Some(1), Some(2), None).unwrap(),
    ]);
    assert_eq!(
      blocks,
      vec![DataBlock::Range(
        BytesPosition::new(None, Some(2), None).unwrap()
      )]
    );
  }

  #[test]
  fn data_block_from_empty_bytes_positions() {
    let blocks =
      DataBlock::from_bytes_positions(vec![BytesPosition::new(Some(0), Some(0), None).unwrap()]);
    assert_eq!(blocks, vec![]);
  }

  #[test]
  fn data_block_is_empty() {
    assert!(DataBlock::Range(BytesPosition::new(Some(0), Some(0), None).unwrap()).is_empty());
    assert!(DataBlock::Data(vec![], None).is_empty());
    assert!(!DataBlock::Range(BytesPosition::new(Some(0), Some(1), None).unwrap()).is_empty());
    assert!(!DataBlock::Data(vec![0], None).is_empty());
  }

  #[test]
  fn byte_range_from_byte_position() {
    let result =
      BytesRange::try_from(&BytesPosition::new(Some(5), Some(10), None).unwrap()).unwrap();
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
      &BytesPosition::new(Some(0), Some(1), None).unwrap()
    );
  }

  #[test]
  fn get_options_with_range() {
    let request_headers = Default::default();
    let result = GetOptions::new_with_default_range(&request_headers)
      .with_range(BytesPosition::new(Some(5), Some(11), Some(Class::Header)).unwrap());
    assert_eq!(
      result.range(),
      &BytesPosition::new(Some(5), Some(11), Some(Class::Header)).unwrap()
    );
  }

  #[test]
  fn url_options_with_range() {
    let request_headers = Default::default();
    let result = RangeUrlOptions::new_with_default_range(&request_headers)
      .with_range(BytesPosition::new(Some(5), Some(11), Some(Class::Header)).unwrap());
    assert_eq!(
      result.range(),
      &BytesPosition::new(Some(5), Some(11), Some(Class::Header)).unwrap()
    );
  }

  #[test]
  fn url_options_apply_with_bytes_range() {
    let result = RangeUrlOptions::new(
      BytesPosition::new(Some(5), Some(11), Some(Class::Header)).unwrap(),
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
      BytesPosition::new(Some(5), Some(11), Some(Class::Header)).unwrap(),
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
