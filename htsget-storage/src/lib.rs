//! Module providing the abstractions needed to read files from an storage
//!

pub use htsget_config::config::{Config, DataServerConfig, ServiceInfo, TicketServerConfig};
pub use htsget_config::resolver::{
  IdResolver, QueryAllowed, ResolveResponse, Resolver, StorageResolver,
};
pub use htsget_config::types::{
  Class, Format, Headers, HtsGetError, JsonResponse, Query, Response, Url,
};

use async_trait::async_trait;
use base64::engine::general_purpose;
use base64::Engine;
use htsget_config::storage::local::LocalStorage as LocalStorageConfig;
#[cfg(feature = "s3-storage")]
use htsget_config::storage::s3::S3Storage as S3StorageConfig;
#[cfg(feature = "url-storage")]
use htsget_config::storage::url::UrlStorageClient as UrlStorageConfig;
use http::{uri, HeaderMap};
use pin_project_lite::pin_project;
use std::cmp::Ordering;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::num::ParseIntError;
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};
use tracing::instrument;

#[cfg(feature = "c4gh-experimental")]
use crate::c4gh::storage::C4GHStorage;
use crate::error::Result;
use crate::error::StorageError;
use crate::local::LocalStorage;
#[cfg(feature = "s3-storage")]
use crate::s3::S3Storage;
#[cfg(feature = "url-storage")]
use crate::url::UrlStorage;
use htsget_config::storage::object::ObjectType;
use htsget_config::types::Scheme;

#[cfg(feature = "c4gh-experimental")]
pub mod c4gh;
pub mod error;
pub mod local;
#[cfg(feature = "s3-storage")]
pub mod s3;
#[cfg(feature = "url-storage")]
pub mod url;

pin_project! {
  /// A Streamable type represents any AsyncRead data used by `StorageTrait`.
  pub struct Streamable {
    #[pin]
    inner: Box<dyn AsyncRead + Send + Sync + Unpin + 'static>,
  }
}

impl Streamable {
  /// Create a new Streamable from an AsyncRead.
  pub fn from_async_read(inner: impl AsyncRead + Send + Sync + Unpin + 'static) -> Self {
    Self {
      inner: Box::new(inner),
    }
  }
}

impl AsyncRead for Streamable {
  fn poll_read(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<std::io::Result<()>> {
    self.project().inner.poll_read(cx, buf)
  }
}

/// The top-level storage type is created from any `StorageTrait`.
pub struct Storage {
  inner: Box<dyn StorageTrait + Send + Sync + 'static>,
}

impl Clone for Storage {
  fn clone(&self) -> Self {
    Self {
      inner: self.inner.clone_box(),
    }
  }
}

impl Debug for Storage {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    write!(f, "Storage")
  }
}

#[async_trait]
impl StorageMiddleware for Storage {
  async fn preprocess(&mut self, _key: &str, _options: GetOptions<'_>) -> Result<()> {
    self.inner.preprocess(_key, _options).await
  }

  async fn postprocess(
    &self,
    key: &str,
    positions_options: BytesPositionOptions<'_>,
  ) -> Result<Vec<DataBlock>> {
    self.inner.postprocess(key, positions_options).await
  }
}

#[async_trait]
impl StorageTrait for Storage {
  async fn get(&self, key: &str, options: GetOptions<'_>) -> Result<Streamable> {
    self.inner.get(key, options).await
  }

  async fn range_url(&self, key: &str, options: RangeUrlOptions<'_>) -> Result<Url> {
    self.inner.range_url(key, options).await
  }

  async fn head(&self, key: &str, options: HeadOptions<'_>) -> Result<u64> {
    self.inner.head(key, options).await
  }

  fn data_url(&self, data: Vec<u8>, class: Option<Class>) -> Url {
    self.inner.data_url(data, class)
  }
}

impl Storage {
  /// Create from local storage config.
  pub async fn from_local(config: &LocalStorageConfig) -> Result<Storage> {
    let storage = LocalStorage::new(config.local_path(), config.clone())?;
    match config.object_type() {
      ObjectType::Regular => Ok(Storage::new(storage)),
      #[cfg(feature = "c4gh-experimental")]
      ObjectType::C4GH { keys } => Ok(Storage::new(C4GHStorage::new(
        keys.clone().into_inner(),
        storage,
      ))),
      _ => Err(StorageError::InternalError(
        "invalid object type".to_string(),
      )),
    }
  }

  /// Create from s3 config.
  #[cfg(feature = "s3-storage")]
  pub async fn from_s3(s3_storage: &S3StorageConfig) -> Storage {
    Storage::new(
      S3Storage::new_with_default_config(
        s3_storage.bucket().to_string(),
        s3_storage.clone().endpoint(),
        s3_storage.clone().path_style(),
      )
      .await,
    )
  }

  /// Create from url config.
  #[cfg(feature = "url-storage")]
  pub async fn from_url(url_storage_config: &UrlStorageConfig) -> Storage {
    Storage::new(UrlStorage::new(
      url_storage_config.client_cloned(),
      url_storage_config.url().clone(),
      url_storage_config.response_url().clone(),
      url_storage_config.forward_headers(),
      url_storage_config.header_blacklist().to_vec(),
    ))
  }

  pub fn new(inner: impl StorageTrait + Send + Sync + 'static) -> Self {
    Self {
      inner: Box::new(inner),
    }
  }
}

/// A Storage represents some kind of object based storage (either locally or in the cloud)
/// that can be used to retrieve files for alignments, variants or its respective indexes.
#[async_trait]
pub trait StorageTrait: StorageMiddleware + StorageClone {
  /// Get the object using the key.
  async fn get(&self, key: &str, options: GetOptions<'_>) -> Result<Streamable>;

  /// Get the url of the object represented by the key using a bytes range. It is not required for
  /// this function to check for the existent of the key, so this should be ensured beforehand.
  async fn range_url(&self, key: &str, options: RangeUrlOptions<'_>) -> Result<Url>;

  /// Get the size of the object represented by the key.
  async fn head(&self, key: &str, options: HeadOptions<'_>) -> Result<u64>;

  /// Get the url of the object using an inline data uri.
  fn data_url(&self, data: Vec<u8>, class: Option<Class>) -> Url {
    Url::new(format!(
      "data:;base64,{}",
      general_purpose::STANDARD.encode(data)
    ))
    .set_class(class)
  }
}

/// Allow the `StorageTrait` to be cloned. This allows cloning a dynamic trait inside a Box.
/// See https://crates.io/crates/dyn-clone for a similar pattern.
pub trait StorageClone {
  fn clone_box(&self) -> Box<dyn StorageTrait + Send + Sync>;
}

impl<T> StorageClone for T
where
  T: StorageTrait + Send + Sync + Clone + 'static,
{
  fn clone_box(&self) -> Box<dyn StorageTrait + Send + Sync> {
    Box::new(self.clone())
  }
}

/// A middleware trait which related to transforming or processing data returned from `StorageTrait`.
#[async_trait]
pub trait StorageMiddleware {
  /// Preprocess any required state before it is requested by `StorageTrait`.
  async fn preprocess(&mut self, _key: &str, _options: GetOptions<'_>) -> Result<()> {
    Ok(())
  }

  /// Postprocess data blocks before they are returned to the client.
  async fn postprocess(
    &self,
    _key: &str,
    positions_options: BytesPositionOptions<'_>,
  ) -> Result<Vec<DataBlock>> {
    Ok(DataBlock::from_bytes_positions(
      positions_options.merge_all().into_inner(),
    ))
  }
}

/// Formats a url for use with storage.
pub trait UrlFormatter {
  /// Returns the url with the path.
  fn format_url<K: AsRef<str>>(&self, key: K) -> Result<String>;
}

impl UrlFormatter for htsget_config::storage::local::LocalStorage {
  fn format_url<K: AsRef<str>>(&self, key: K) -> Result<String> {
    uri::Builder::new()
      .scheme(match self.scheme() {
        Scheme::Http => uri::Scheme::HTTP,
        Scheme::Https => uri::Scheme::HTTPS,
      })
      .authority(self.authority().to_string())
      .path_and_query(format!("{}/{}", self.path_prefix(), key.as_ref()))
      .build()
      .map_err(|err| StorageError::InvalidUri(err.to_string()))
      .map(|value| value.to_string())
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
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    let start = self
      .start
      .map(|start| start.to_string())
      .unwrap_or_else(|| "0".to_string());
    let end = self.end.map(|end| end.to_string()).unwrap_or_default();
    write!(f, "bytes={start}-{end}")
  }
}

/// Convert from a http range to a bytes position.
impl FromStr for BytesPosition {
  type Err = StorageError;

  fn from_str(range: &str) -> Result<Self> {
    let range = range.replacen("bytes=", "", 1);

    let split: Vec<&str> = range.splitn(2, '-').collect();
    if split.len() > 2 {
      return Err(StorageError::InternalError(
        "failed to split range".to_string(),
      ));
    }

    let parse_range = |range: Option<&str>| {
      let range = range.unwrap_or_default();
      if range.is_empty() {
        Ok::<_, Self::Err>(None)
      } else {
        Ok(Some(range.parse().map_err(|err: ParseIntError| {
          StorageError::InternalError(err.to_string())
        })?))
      }
    };

    let start = parse_range(split.first().copied())?;
    let end = parse_range(split.last().copied())?.map(|value| value + 1);

    Ok(Self::new(start, end, None))
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

#[derive(Debug, Clone)]
pub struct GetOptions<'a> {
  range: BytesPosition,
  request_headers: &'a HeaderMap,
}

impl<'a> GetOptions<'a> {
  pub fn new(range: BytesPosition, request_headers: &'a HeaderMap) -> Self {
    Self {
      range,
      request_headers,
    }
  }

  pub fn new_with_default_range(request_headers: &'a HeaderMap) -> Self {
    Self::new(Default::default(), request_headers)
  }

  pub fn with_max_length(mut self, max_length: u64) -> Self {
    self.range = BytesPosition::default().with_start(0).with_end(max_length);
    self
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
  pub fn request_headers(&self) -> &'a HeaderMap {
    self.request_headers
  }
}

#[derive(Debug, Clone)]
pub struct BytesPositionOptions<'a> {
  positions: Vec<BytesPosition>,
  headers: &'a HeaderMap,
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

  pub fn apply(self, url: Url) -> Url {
    let range: String = String::from(&BytesRange::from(self.range()));

    let url = if range.is_empty() {
      url
    } else {
      url.add_headers(Headers::default().with_header("Range", range))
    };

    url.set_class(self.range().class)
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

  use http::uri::Authority;

  use crate::local::LocalStorage;
  use htsget_config::storage::local::LocalStorage as ConfigLocalStorage;
  use htsget_test::util::default_dir;

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
    let result = LocalStorage::<ConfigLocalStorage>::new(
      default_dir().join("data"),
      ConfigLocalStorage::default(),
    )
    .unwrap()
    .data_url(b"Hello World!".to_vec(), Some(Class::Header));
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
      assert!(class.is_some());
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
      assert!(class.is_none());
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
    let request_headers = Default::default();
    let result = GetOptions::new_with_default_range(&request_headers).with_max_length(1);
    assert_eq!(
      result.range(),
      &BytesPosition::default().with_start(0).with_end(1)
    );
  }

  #[test]
  fn get_options_with_range() {
    let request_headers = Default::default();
    let result = GetOptions::new_with_default_range(&request_headers)
      .with_range(BytesPosition::new(Some(5), Some(11), Some(Class::Header)));
    assert_eq!(
      result.range(),
      &BytesPosition::new(Some(5), Some(11), Some(Class::Header))
    );
  }

  #[test]
  fn url_options_with_range() {
    let request_headers = Default::default();
    let result = RangeUrlOptions::new_with_default_range(&request_headers)
      .with_range(BytesPosition::new(Some(5), Some(11), Some(Class::Header)));
    assert_eq!(
      result.range(),
      &BytesPosition::new(Some(5), Some(11), Some(Class::Header))
    );
  }

  #[test]
  fn url_options_apply_with_bytes_range() {
    let result = RangeUrlOptions::new(
      BytesPosition::new(Some(5), Some(11), Some(Class::Header)),
      &Default::default(),
    )
    .apply(Url::new(""));
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
    let result = RangeUrlOptions::new_with_default_range(&Default::default()).apply(Url::new(""));
    assert_eq!(result, Url::new(""));
  }

  #[test]
  fn url_options_apply_with_headers() {
    let result = RangeUrlOptions::new(
      BytesPosition::new(Some(5), Some(11), Some(Class::Header)),
      &Default::default(),
    )
    .apply(Url::new("").with_headers(Headers::default().with_header("header", "value")));
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

  #[test]
  fn http_formatter_authority() {
    let formatter = ConfigLocalStorage::new(
      Scheme::Http,
      Authority::from_static("127.0.0.1:8080"),
      "data".to_string(),
      "/data".to_string(),
      Default::default(),
    );
    test_formatter_authority(formatter, "http");
  }

  #[test]
  fn https_formatter_authority() {
    let formatter = ConfigLocalStorage::new(
      Scheme::Https,
      Authority::from_static("127.0.0.1:8080"),
      "data".to_string(),
      "/data".to_string(),
      Default::default(),
    );
    test_formatter_authority(formatter, "https");
  }

  #[test]
  fn htsget_error_from_storage_not_found() {
    let result = HtsGetError::from(StorageError::KeyNotFound("error".to_string()));
    assert!(matches!(result, HtsGetError::NotFound(_)));
  }

  #[test]
  fn htsget_error_from_storage_invalid_key() {
    let result = HtsGetError::from(StorageError::InvalidKey("error".to_string()));
    assert!(matches!(result, HtsGetError::NotFound(_)));
  }

  fn test_formatter_authority(formatter: ConfigLocalStorage, scheme: &str) {
    assert_eq!(
      formatter.format_url("path").unwrap(),
      format!("{}://127.0.0.1:8080{}/path", scheme, "/data")
    )
  }
}
