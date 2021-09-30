//! Module providing the abstractions needed to read files from an storage
//!
use std::cmp::Ordering;

use thiserror::Error;

#[cfg(feature = "async")]
pub use async_storage::*;
#[cfg(feature = "aws")]
pub mod aws;
#[cfg(feature = "aws")]
use rusoto_core::RusotoError;
#[cfg(feature = "aws")]
use rusoto_s3::HeadObjectError;

use crate::htsget::Class;

#[cfg(feature = "async")]
pub mod async_storage;
pub mod blocking;
#[cfg(feature = "async")]
pub mod local;
// #[cfg(feature = "aws")]
// pub mod aws;

type Result<T> = core::result::Result<T, StorageError>;

#[derive(Error, PartialEq, Debug)]
pub enum StorageError {
  #[error("Invalid key: {0}")]
  InvalidKey(String),

  #[error("Not found: {0}")]
  NotFound(String),

  #[cfg(feature = "aws")]
  #[error("AwsError")]
  AwsError {
    #[from]
    source: RusotoError<HeadObjectError>,
  },
}

#[derive(Debug, Clone, PartialEq)]
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

  pub fn merge_with(&mut self, range: &BytesRange) -> &Self {
    self.start = match (self.start.as_ref(), range.start.as_ref()) {
      (None, None) | (None, Some(_)) | (Some(_), None) => None,
      (Some(a), Some(b)) => Some(*a.min(b)),
    };
    self.end = match (self.end.as_ref(), range.end.as_ref()) {
      (None, None) | (None, Some(_)) | (Some(_), None) => None,
      (Some(a), Some(b)) => Some(*a.max(b)),
    };
    self
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
          current_range.merge_with(range);
        } else {
          optimized_ranges.push(current_range.clone());
          current_range = range.clone();
        }
      }

      optimized_ranges.push(current_range);

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
  class: Class,
}

impl UrlOptions {
  pub fn with_range(mut self, range: BytesRange) -> Self {
    self.range = range;
    self
  }

  pub fn with_class(mut self, class: Class) -> Self {
    self.class = class;
    self
  }
}

impl Default for UrlOptions {
  fn default() -> Self {
    Self {
      range: BytesRange::default(),
      class: Class::Body,
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::htsget::Class;

  use super::*;

  #[test]
  fn bytes_range_overlapping_and_merge() {
    let test_cases = vec![
      (
        //                                             0123456789
        BytesRange::new(None, Some(2)),    //         <--]
        BytesRange::new(Some(3), Some(5)), //             [-]
        None,
      ),
      (
        //                                             0123456789
        BytesRange::new(None, Some(2)), //            <--]
        BytesRange::new(Some(3), None), //                [------>
        None,
      ),
      (
        //                                             0123456789
        BytesRange::new(None, Some(2)),       //      <--]
        BytesRange::new(Some(2), Some(4)),    //         [-]
        Some(BytesRange::new(None, Some(4))), //      <----]
      ),
      (
        //                                             0123456789
        BytesRange::new(None, Some(2)),    //         <--]
        BytesRange::new(Some(2), None),    //            [------->
        Some(BytesRange::new(None, None)), //         <---------->
      ),
      (
        //                                             0123456789
        BytesRange::new(None, Some(2)),       //      <--]
        BytesRange::new(Some(1), Some(3)),    //        [-]
        Some(BytesRange::new(None, Some(3))), //      <---]
      ),
      (
        //                                             0123456789
        BytesRange::new(None, Some(2)),    //         <--]
        BytesRange::new(Some(1), None),    //           [-------->
        Some(BytesRange::new(None, None)), //         <---------->
      ),
      (
        //                                             0123456789
        BytesRange::new(None, Some(2)),       //      <--]
        BytesRange::new(Some(0), Some(2)),    //       [-]
        Some(BytesRange::new(None, Some(2))), //      <--]
      ),
      (
        //                                             0123456789
        BytesRange::new(None, Some(2)),       //      <--]
        BytesRange::new(None, Some(2)),       //      <--]
        Some(BytesRange::new(None, Some(2))), //      <--]
      ),
      (
        //                                             0123456789
        BytesRange::new(None, Some(2)),       //      <--]
        BytesRange::new(Some(0), Some(1)),    //       []
        Some(BytesRange::new(None, Some(2))), //      <--]
      ),
      (
        //                                             0123456789
        BytesRange::new(None, Some(2)),       //      <--]
        BytesRange::new(None, Some(1)),       //      <-]
        Some(BytesRange::new(None, Some(2))), //      <--]
      ),
      (
        //                                             0123456789
        BytesRange::new(None, Some(2)),    //         <--]
        BytesRange::new(None, None),       //         <---------->
        Some(BytesRange::new(None, None)), //         <---------->
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)), //            [-]
        BytesRange::new(Some(6), Some(8)), //                [-]
        None,
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)), //            [-]
        BytesRange::new(Some(6), None),    //                [--->
        None,
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)),       //      [-]
        BytesRange::new(Some(4), Some(6)),       //        [-]
        Some(BytesRange::new(Some(2), Some(6))), //      [---]
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)),    //         [-]
        BytesRange::new(Some(4), None),       //           [----->
        Some(BytesRange::new(Some(2), None)), //         [------->
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)),       //      [-]
        BytesRange::new(Some(3), Some(5)),       //       [-]
        Some(BytesRange::new(Some(2), Some(5))), //      [--]
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)),    //         [-]
        BytesRange::new(Some(3), None),       //          [------>
        Some(BytesRange::new(Some(2), None)), //         [------->
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)),       //      [-]
        BytesRange::new(Some(2), Some(3)),       //      []
        Some(BytesRange::new(Some(2), Some(4))), //      [-]
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)),    //         [-]
        BytesRange::new(None, Some(3)),       //      <---]
        Some(BytesRange::new(None, Some(4))), //      <----]
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)),       //      [-]
        BytesRange::new(Some(1), Some(3)),       //     [-]
        Some(BytesRange::new(Some(1), Some(4))), //     [--]
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)),    //         [-]
        BytesRange::new(None, Some(3)),       //      <---]
        Some(BytesRange::new(None, Some(4))), //      <----]
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)),       //      [-]
        BytesRange::new(Some(0), Some(2)),       //    [-]
        Some(BytesRange::new(Some(0), Some(4))), //    [---]
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)),    //         [-]
        BytesRange::new(None, Some(2)),       //      <--]
        Some(BytesRange::new(None, Some(4))), //      <----]
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)), //            [-]
        BytesRange::new(Some(0), Some(1)), //          []
        None,
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)), //            [-]
        BytesRange::new(None, Some(1)),    //         <-]
        None,
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), Some(4)), //            [-]
        BytesRange::new(None, None),       //         <---------->
        Some(BytesRange::new(None, None)), //         <---------->
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), None),       //         [------->
        BytesRange::new(Some(4), Some(6)),    //           [-]
        Some(BytesRange::new(Some(2), None)), //         [------->
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), None),       //         [------->
        BytesRange::new(Some(4), None),       //           [----->
        Some(BytesRange::new(Some(2), None)), //         [------->
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), None),       //         [------->
        BytesRange::new(Some(2), Some(4)),    //         [-]
        Some(BytesRange::new(Some(2), None)), //         [------->
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), None),       //         [------->
        BytesRange::new(Some(2), None),       //         [------->
        Some(BytesRange::new(Some(2), None)), //         [------->
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), None),       //         [------->
        BytesRange::new(Some(1), Some(3)),    //        [-]
        Some(BytesRange::new(Some(1), None)), //        [-------->
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), None),    //            [------->
        BytesRange::new(None, Some(3)),    //         <---]
        Some(BytesRange::new(None, None)), //         <---------->
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), None),       //         [------->
        BytesRange::new(Some(0), Some(2)),    //       [-]
        Some(BytesRange::new(Some(0), None)), //       [--------->
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), None),    //            [------->
        BytesRange::new(None, Some(2)),    //         <--]
        Some(BytesRange::new(None, None)), //         <---------->
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), None),    //            [------->
        BytesRange::new(Some(0), Some(1)), //          []
        None,
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), None), //               [------->
        BytesRange::new(None, Some(1)), //            <-]
        None,
      ),
      (
        //                                             0123456789
        BytesRange::new(Some(2), None),    //            [------->
        BytesRange::new(None, None),       //         <---------->
        Some(BytesRange::new(None, None)), //         <---------->
      ),
      (
        //                                             0123456789
        BytesRange::new(None, None),       //         <---------->
        BytesRange::new(None, None),       //         <---------->
        Some(BytesRange::new(None, None)), //         <---------->
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
    assert_eq!(BytesRange::merge_all(Vec::new()), Vec::new());
  }

  #[test]
  fn bytes_range_merge_all_when_list_has_one_range() {
    assert_eq!(
      BytesRange::merge_all(vec![BytesRange::default()]),
      vec![BytesRange::default()]
    );
  }

  #[test]
  fn bytes_range_merge_all_when_list_has_many_ranges() {
    let ranges = vec![
      BytesRange::new(None, Some(1)),
      BytesRange::new(Some(1), Some(2)),
      BytesRange::new(Some(5), Some(6)),
      BytesRange::new(Some(5), Some(8)),
      BytesRange::new(Some(6), Some(7)),
      BytesRange::new(Some(4), Some(5)),
      BytesRange::new(Some(3), Some(6)),
      BytesRange::new(Some(10), Some(12)),
      BytesRange::new(Some(10), Some(12)),
      BytesRange::new(Some(10), Some(14)),
      BytesRange::new(Some(14), Some(15)),
      BytesRange::new(Some(12), Some(16)),
      BytesRange::new(Some(17), Some(19)),
      BytesRange::new(Some(21), Some(23)),
      BytesRange::new(Some(18), Some(22)),
      BytesRange::new(Some(24), None),
      BytesRange::new(Some(24), Some(30)),
      BytesRange::new(Some(31), Some(33)),
      BytesRange::new(Some(35), None),
    ];

    let expected_ranges = vec![
      BytesRange::new(None, Some(2)),
      BytesRange::new(Some(3), Some(8)),
      BytesRange::new(Some(10), Some(16)),
      BytesRange::new(Some(17), Some(23)),
      BytesRange::new(Some(24), None),
    ];

    assert_eq!(BytesRange::merge_all(ranges), expected_ranges);
  }

  #[test]
  fn get_options_with_max_length() {
    let result = GetOptions::default().with_max_length(1);
    assert_eq!(
      result.range,
      BytesRange::default().with_start(0).with_end(1)
    );
  }

  #[test]
  fn get_options_with_range() {
    let result = GetOptions::default().with_range(BytesRange::default());
    assert_eq!(result.range, BytesRange::default());
  }

  #[test]
  fn url_options_with_range() {
    let result = UrlOptions::default().with_range(BytesRange::default());
    assert_eq!(result.range, BytesRange::default());
    assert_eq!(result.class, Class::Body);
  }

  #[test]
  fn url_options_with_class() {
    let result = UrlOptions::default().with_class(Class::Header);
    assert_eq!(result.range, BytesRange::default());
    assert_eq!(result.class, Class::Header);
  }
}
