//! Module providing the abstractions needed to read files from an storage
//!
use std::cmp::Ordering;
use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::io::ErrorKind;
use std::net::AddrParseError;

use async_trait::async_trait;
use base64::encode;
use thiserror::Error;
use tokio::io::AsyncRead;
use tracing::instrument;

use htsget_config::regex_resolver::{HtsGetIdResolver, RegexResolver};

use crate::htsget::{Class, Headers, Url};

#[cfg(feature = "s3-storage")]
pub mod aws;
pub mod data_server;
pub mod local;

type Result<T> = core::result::Result<T, StorageError>;

/// A Storage represents some kind of object based storage (either locally or in the cloud)
/// that can be used to retrieve files for alignments, variants or its respective indexes.
#[async_trait]
pub trait Storage {
  type Streamable: AsyncRead + Unpin + Send;

  /// Get the object using the key.
  async fn get<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: GetOptions,
  ) -> Result<Self::Streamable>;

  /// Get the url of the object represented by the key using a bytes range.
  async fn range_url<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: RangeUrlOptions,
  ) -> Result<Url>;

  /// Get the size of the object represented by the key.
  async fn head<K: AsRef<str> + Send + Debug>(&self, key: K) -> Result<u64>;

  /// Get the url of the object using an inline data uri.
  #[instrument(level = "trace", ret)]
  fn data_url(data: Vec<u8>, class: Option<Class>) -> Url {
    Url::new(format!("data:;base64,{}", encode(data))).set_class(class)
  }
}

/// Formats a url for use with storage.
pub trait UrlFormatter {
  /// Returns the url with the path.
  fn format_url<K: AsRef<str>>(&self, key: K) -> Result<String>;
}

#[derive(Error, Debug)]
pub enum StorageError {
  #[error("wrong key derived from ID: `{0}`")]
  InvalidKey(String),

  #[error("key not found in storage: `{0}`")]
  KeyNotFound(String),

  #[error("{0}: {1}")]
  IoError(String, io::Error),

  #[error("url response ticket server error: {0}")]
  DataServerError(String),

  #[error("invalid input: {0}")]
  InvalidInput(String),

  #[error("invalid uri: {0}")]
  InvalidUri(String),

  #[error("invalid address: {0}")]
  InvalidAddress(AddrParseError),

  #[error("internal error: {0}")]
  InternalError(String),

  #[cfg(feature = "s3-storage")]
  #[error("aws error: {0}, with key: `{1}`")]
  AwsS3Error(String, String),
}

impl From<StorageError> for io::Error {
  fn from(err: StorageError) -> Self {
    match err {
      StorageError::IoError(_, ref io_error) => Self::new(io_error.kind(), err),
      err => Self::new(ErrorKind::Other, err),
    }
  }
}

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
#[derive(Clone, Debug, Default, PartialEq)]
struct BytesRange {
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
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    let start = self
      .start
      .map(|start| start.to_string())
      .unwrap_or_else(|| "0".to_string());
    let end = self
      .end
      .map(|end| end.to_string())
      .unwrap_or_else(|| "".to_string());
    write!(f, "bytes={}-{}", start, end)
  }
}

impl From<&BytesPosition> for BytesRange {
  fn from(pos: &BytesPosition) -> Self {
    Self::new(pos.start, pos.end.map(|value| value - 1))
  }
}

impl BytesRange {
  pub fn new(start: Option<u64>, end: Option<u64>) -> Self {
    Self { start, end }
  }
}

impl BytesPosition {
  pub fn new(start: Option<u64>, end: Option<u64>, class: Option<Class>) -> Self {
    Self { start, end, class }
  }

  pub fn with_start(mut self, start: u64) -> Self {
    self.start = Some(start);
    self
  }

  pub fn with_end(mut self, end: u64) -> Self {
    self.end = Some(end);
    self
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

#[derive(Debug, Default)]
pub struct GetOptions {
  range: BytesPosition,
}

impl GetOptions {
  pub fn with_max_length(mut self, max_length: u64) -> Self {
    self.range = BytesPosition::default().with_start(0).with_end(max_length);
    self
  }

  pub fn with_range(mut self, range: BytesPosition) -> Self {
    self.range = range;
    self
  }
}

#[derive(Debug, Default)]
pub struct RangeUrlOptions {
  range: BytesPosition,
}

impl From<BytesPosition> for RangeUrlOptions {
  fn from(bytes_position: BytesPosition) -> Self {
    Self::default().with_range(bytes_position)
  }
}

impl RangeUrlOptions {
  pub fn with_range(mut self, range: BytesPosition) -> Self {
    self.range = range;
    self
  }

  pub fn apply(self, url: Url) -> Url {
    let range: String = String::from(&BytesRange::from(&self.range));
    let url = if range.is_empty() {
      url
    } else {
      url.with_headers(Headers::default().with_header("Range", range))
    };
    url.set_class(self.range.class)
  }
}

/// Resolve a key id with the `RegexResolver` and convert it to a Result.
fn resolve_id<K: AsRef<str>>(resolver: &RegexResolver, key: &K) -> Result<String> {
  resolver
    .resolve_id(key.as_ref())
    .ok_or_else(|| StorageError::InvalidKey(key.as_ref().to_string()))
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use crate::htsget::Class;
  use crate::storage::data_server::HttpTicketFormatter;
  use crate::storage::local::LocalStorage;

  use super::*;

  #[test]
  fn bytes_range_overlapping_and_merge() {
    let test_cases = vec![
      (
        BytesPosition::new(None, Some(2), None),
        BytesPosition::new(Some(3), Some(5), None),
        None,
      ),
      (
        BytesPosition::new(None, Some(2), None),
        BytesPosition::new(Some(3), None, None),
        None,
      ),
      (
        BytesPosition::new(None, Some(2), None),
        BytesPosition::new(Some(2), Some(4), None),
        Some(BytesPosition::new(None, Some(4), None)),
      ),
      (
        BytesPosition::new(None, Some(2), None),
        BytesPosition::new(Some(2), None, None),
        Some(BytesPosition::new(None, None, None)),
      ),
      (
        BytesPosition::new(None, Some(2), None),
        BytesPosition::new(Some(1), Some(3), None),
        Some(BytesPosition::new(None, Some(3), None)),
      ),
      (
        BytesPosition::new(None, Some(2), None),
        BytesPosition::new(Some(1), None, None),
        Some(BytesPosition::new(None, None, None)),
      ),
      (
        BytesPosition::new(None, Some(2), None),
        BytesPosition::new(Some(0), Some(2), None),
        Some(BytesPosition::new(None, Some(2), None)),
      ),
      (
        BytesPosition::new(None, Some(2), None),
        BytesPosition::new(None, Some(2), None),
        Some(BytesPosition::new(None, Some(2), None)),
      ),
      (
        BytesPosition::new(None, Some(2), None),
        BytesPosition::new(Some(0), Some(1), None),
        Some(BytesPosition::new(None, Some(2), None)),
      ),
      (
        BytesPosition::new(None, Some(2), None),
        BytesPosition::new(None, Some(1), None),
        Some(BytesPosition::new(None, Some(2), None)),
      ),
      (
        BytesPosition::new(None, Some(2), None),
        BytesPosition::new(None, None, None),
        Some(BytesPosition::new(None, None, None)),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(Some(6), Some(8), None),
        None,
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(Some(6), None, None),
        None,
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(Some(4), Some(6), None),
        Some(BytesPosition::new(Some(2), Some(6), None)),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(Some(4), None, None),
        Some(BytesPosition::new(Some(2), None, None)),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(Some(3), Some(5), None),
        Some(BytesPosition::new(Some(2), Some(5), None)),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(Some(3), None, None),
        Some(BytesPosition::new(Some(2), None, None)),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(Some(2), Some(3), None),
        Some(BytesPosition::new(Some(2), Some(4), None)),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(None, Some(3), None),
        Some(BytesPosition::new(None, Some(4), None)),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(Some(1), Some(3), None),
        Some(BytesPosition::new(Some(1), Some(4), None)),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(None, Some(3), None),
        Some(BytesPosition::new(None, Some(4), None)),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(Some(0), Some(2), None),
        Some(BytesPosition::new(Some(0), Some(4), None)),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(None, Some(2), None),
        Some(BytesPosition::new(None, Some(4), None)),
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(Some(0), Some(1), None),
        None,
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(None, Some(1), None),
        None,
      ),
      (
        BytesPosition::new(Some(2), Some(4), None),
        BytesPosition::new(None, None, None),
        Some(BytesPosition::new(None, None, None)),
      ),
      (
        BytesPosition::new(Some(2), None, None),
        BytesPosition::new(Some(4), Some(6), None),
        Some(BytesPosition::new(Some(2), None, None)),
      ),
      (
        BytesPosition::new(Some(2), None, None),
        BytesPosition::new(Some(4), None, None),
        Some(BytesPosition::new(Some(2), None, None)),
      ),
      (
        BytesPosition::new(Some(2), None, None),
        BytesPosition::new(Some(2), Some(4), None),
        Some(BytesPosition::new(Some(2), None, None)),
      ),
      (
        BytesPosition::new(Some(2), None, None),
        BytesPosition::new(Some(2), None, None),
        Some(BytesPosition::new(Some(2), None, None)),
      ),
      (
        BytesPosition::new(Some(2), None, None),
        BytesPosition::new(Some(1), Some(3), None),
        Some(BytesPosition::new(Some(1), None, None)),
      ),
      (
        BytesPosition::new(Some(2), None, None),
        BytesPosition::new(None, Some(3), None),
        Some(BytesPosition::new(None, None, None)),
      ),
      (
        BytesPosition::new(Some(2), None, None),
        BytesPosition::new(Some(0), Some(2), None),
        Some(BytesPosition::new(Some(0), None, None)),
      ),
      (
        BytesPosition::new(Some(2), None, None),
        BytesPosition::new(None, Some(2), None),
        Some(BytesPosition::new(None, None, None)),
      ),
      (
        BytesPosition::new(Some(2), None, None),
        BytesPosition::new(Some(0), Some(1), None),
        None,
      ),
      (
        BytesPosition::new(Some(2), None, None),
        BytesPosition::new(None, Some(1), None),
        None,
      ),
      (
        BytesPosition::new(Some(2), None, None),
        BytesPosition::new(None, None, None),
        Some(BytesPosition::new(None, None, None)),
      ),
      (
        BytesPosition::new(None, None, None),
        BytesPosition::new(None, None, None),
        Some(BytesPosition::new(None, None, None)),
      ),
    ];

    for (index, (a, b, expected)) in test_cases.iter().enumerate() {
      println!("Test case {}", index);
      println!("  {:?}", a);
      println!("  {:?}", b);
      println!("  {:?}", expected);

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
        BytesPosition::new(None, Some(1), Some(Class::Header)),
        BytesPosition::new(None, Some(2), Some(Class::Header))
      ]),
      vec![BytesPosition::new(None, Some(2), Some(Class::Header))]
    );
  }

  #[test]
  fn bytes_position_merge_class_body() {
    assert_eq!(
      BytesPosition::merge_all(vec![
        BytesPosition::new(None, Some(1), Some(Class::Body)),
        BytesPosition::new(None, Some(3), Some(Class::Body))
      ]),
      vec![BytesPosition::new(None, Some(3), Some(Class::Body))]
    );
  }

  #[test]
  fn bytes_position_merge_class_none() {
    assert_eq!(
      BytesPosition::merge_all(vec![
        BytesPosition::new(Some(1), Some(2), None),
        BytesPosition::new(Some(2), Some(3), None)
      ]),
      vec![BytesPosition::new(Some(1), Some(3), None)]
    );
  }

  #[test]
  fn bytes_position_merge_class_different() {
    assert_eq!(
      BytesPosition::merge_all(vec![
        BytesPosition::new(Some(1), Some(2), Some(Class::Header)),
        BytesPosition::new(Some(2), Some(3), Some(Class::Body))
      ]),
      vec![BytesPosition::new(Some(1), Some(3), None)]
    );
  }

  #[test]
  fn bytes_range_merge_all_when_list_has_many_ranges() {
    let ranges = vec![
      BytesPosition::new(None, Some(1), None),
      BytesPosition::new(Some(1), Some(2), None),
      BytesPosition::new(Some(5), Some(6), None),
      BytesPosition::new(Some(5), Some(8), None),
      BytesPosition::new(Some(6), Some(7), None),
      BytesPosition::new(Some(4), Some(5), None),
      BytesPosition::new(Some(3), Some(6), None),
      BytesPosition::new(Some(10), Some(12), None),
      BytesPosition::new(Some(10), Some(12), None),
      BytesPosition::new(Some(10), Some(14), None),
      BytesPosition::new(Some(14), Some(15), None),
      BytesPosition::new(Some(12), Some(16), None),
      BytesPosition::new(Some(17), Some(19), None),
      BytesPosition::new(Some(21), Some(23), None),
      BytesPosition::new(Some(18), Some(22), None),
      BytesPosition::new(Some(24), None, None),
      BytesPosition::new(Some(24), Some(30), None),
      BytesPosition::new(Some(31), Some(33), None),
      BytesPosition::new(Some(35), None, None),
    ];

    let expected_ranges = vec![
      BytesPosition::new(None, Some(2), None),
      BytesPosition::new(Some(3), Some(8), None),
      BytesPosition::new(Some(10), Some(16), None),
      BytesPosition::new(Some(17), Some(23), None),
      BytesPosition::new(Some(24), None, None),
    ];

    assert_eq!(BytesPosition::merge_all(ranges), expected_ranges);
  }

  #[test]
  fn bytes_position_new() {
    let result = BytesPosition::new(Some(1), Some(2), Some(Class::Header));
    assert_eq!(result.start, Some(1));
    assert_eq!(result.end, Some(2));
    assert_eq!(result.class, Some(Class::Header));
  }

  #[test]
  fn bytes_position_with_start() {
    let result = BytesPosition::default().with_start(1);
    assert_eq!(result.start, Some(1));
  }

  #[test]
  fn bytes_position_with_end() {
    let result = BytesPosition::default().with_end(1);
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
  fn data_url() {
    let result =
      LocalStorage::<HttpTicketFormatter>::data_url(b"Hello World!".to_vec(), Some(Class::Header));
    let url = data_url::DataUrl::process(&result.url);
    let (result, _) = url.unwrap().decode_to_vec().unwrap();
    assert_eq!(result, b"Hello World!");
  }

  #[test]
  fn data_block_update_classes_all_some() {
    let blocks = DataBlock::update_classes(vec![
      DataBlock::Range(BytesPosition::new(None, Some(1), Some(Class::Body))),
      DataBlock::Data(vec![], Some(Class::Header)),
    ]);
    for block in blocks {
      let class = match block {
        DataBlock::Range(pos) => pos.class,
        DataBlock::Data(_, class) => class,
      };
      assert!(matches!(class, Some(_)));
    }
  }

  #[test]
  fn data_block_update_classes_one_none() {
    let blocks = DataBlock::update_classes(vec![
      DataBlock::Range(BytesPosition::new(None, Some(1), Some(Class::Body))),
      DataBlock::Data(vec![], None),
    ]);
    for block in blocks {
      let class = match block {
        DataBlock::Range(pos) => pos.class,
        DataBlock::Data(_, class) => class,
      };
      assert!(matches!(class, None));
    }
  }

  #[test]
  fn data_block_from_bytes_positions() {
    let blocks = DataBlock::from_bytes_positions(vec![
      BytesPosition::new(None, Some(1), None),
      BytesPosition::new(Some(1), Some(2), None),
    ]);
    assert_eq!(
      blocks,
      vec![DataBlock::Range(BytesPosition::new(None, Some(2), None))]
    );
  }

  #[test]
  fn byte_range_from_byte_position() {
    let result: BytesRange = BytesRange::from(&BytesPosition::default().with_start(5).with_end(10));
    let expected = BytesRange::new(Some(5), Some(9));
    assert_eq!(result, expected);
  }

  #[test]
  fn get_options_with_max_length() {
    let result = GetOptions::default().with_max_length(1);
    assert_eq!(
      result.range,
      BytesPosition::default().with_start(0).with_end(1)
    );
  }

  #[test]
  fn get_options_with_range() {
    let result = GetOptions::default().with_range(BytesPosition::default());
    assert_eq!(result.range, BytesPosition::default());
  }

  #[test]
  fn url_options_with_range() {
    let result = RangeUrlOptions::default().with_range(BytesPosition::default());
    assert_eq!(result.range, BytesPosition::default());
  }

  #[test]
  fn url_options_apply_with_bytes_range() {
    let result = RangeUrlOptions::default()
      .with_range(BytesPosition::new(Some(5), Some(11), Some(Class::Header)))
      .apply(Url::new(""));
    println!("{:?}", result);
    assert_eq!(
      result,
      Url::new("")
        .with_headers(Headers::new(HashMap::new()).with_header("Range", "bytes=5-10"))
        .with_class(Class::Header)
    );
  }

  #[test]
  fn url_options_apply_no_bytes_range() {
    let result = RangeUrlOptions::default().apply(Url::new(""));
    assert_eq!(result, Url::new(""));
  }
}
