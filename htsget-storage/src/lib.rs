//! Module providing the abstractions needed to read files from an storage
//!

pub use htsget_config::resolver::{IdResolver, ResolveResponse, StorageResolver};
pub use htsget_config::types::{
  Class, Format, Headers, HtsGetError, JsonResponse, Query, Response, Url,
};

#[cfg(feature = "experimental")]
use crate::c4gh::storage::C4GHStorage;
use crate::error::Result;
use crate::error::StorageError;
use crate::error::StorageError::InvalidKey;
use crate::local::FileStorage;
#[cfg(feature = "s3-storage")]
use crate::s3::S3Storage;
use crate::types::{BytesPositionOptions, DataBlock, GetOptions, HeadOptions, RangeUrlOptions};
#[cfg(feature = "url-storage")]
use crate::url::UrlStorage;
use async_trait::async_trait;
use base64::engine::general_purpose;
use base64::Engine;
use cfg_if::cfg_if;
use htsget_config::storage;
#[cfg(feature = "experimental")]
use htsget_config::storage::c4gh::C4GHKeys;
use htsget_config::types::Scheme;
use http::uri;
use pin_project_lite::pin_project;
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

#[cfg(feature = "experimental")]
pub mod c4gh;
pub mod error;
pub mod local;
#[cfg(feature = "s3-storage")]
pub mod s3;
pub mod types;
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

impl Storage {
  /// Get the inner value.
  pub fn into_inner(self) -> Box<dyn StorageTrait + Send + Sync> {
    self.inner
  }
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
  #[cfg(feature = "experimental")]
  /// Wrap an existing storage with C4GH storage
  pub async fn from_c4gh_keys(keys: Option<&C4GHKeys>, storage: Storage) -> Result<Storage> {
    if let Some(keys) = keys {
      Ok(Storage::new(C4GHStorage::new_box(
        keys
          .clone()
          .keys()
          .await
          .map_err(|err| StorageError::InternalError(err.to_string()))?,
        storage.into_inner(),
      )))
    } else {
      Ok(storage)
    }
  }

  /// Create from local storage config.
  pub async fn from_file(file: &storage::file::File) -> Result<Storage> {
    let storage = Storage::new(FileStorage::new(file.local_path(), file.clone())?);

    cfg_if! {
      if #[cfg(feature = "experimental")] {
        Self::from_c4gh_keys(file.keys(), storage).await
      } else {
        Ok(storage)
      }
    }
  }

  /// Create from s3 config.
  #[cfg(feature = "s3-storage")]
  pub async fn from_s3(s3: &storage::s3::S3) -> Result<Storage> {
    let storage = Storage::new(
      S3Storage::new_with_default_config(
        s3.bucket().to_string(),
        s3.endpoint().map(str::to_string),
        s3.path_style(),
      )
      .await,
    );

    cfg_if! {
      if #[cfg(feature = "experimental")] {
        Self::from_c4gh_keys(s3.keys(), storage).await
      } else {
        Ok(storage)
      }
    }
  }

  /// Create from url config.
  #[cfg(feature = "url-storage")]
  pub async fn from_url(url: &storage::url::Url) -> Result<Storage> {
    let storage = Storage::new(UrlStorage::new(
      url.client_cloned(),
      url.url().clone(),
      url.response_url().clone(),
      url.forward_headers(),
      url.header_blacklist().to_vec(),
    ));

    cfg_if! {
      if #[cfg(feature = "experimental")] {
        Self::from_c4gh_keys(url.keys(), storage).await
      } else {
        Ok(storage)
      }
    }
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

impl UrlFormatter for storage::file::File {
  fn format_url<K: AsRef<str>>(&self, key: K) -> Result<String> {
    let path = Path::new("/").join(key.as_ref());
    uri::Builder::new()
      .scheme(match self.scheme() {
        Scheme::Http => uri::Scheme::HTTP,
        Scheme::Https => uri::Scheme::HTTPS,
      })
      .authority(self.authority().to_string())
      .path_and_query(
        path
          .to_str()
          .ok_or_else(|| InvalidKey("constructing url".to_string()))?,
      )
      .build()
      .map_err(|err| StorageError::InvalidUri(err.to_string()))
      .map(|value| value.to_string())
  }
}

#[cfg(test)]
mod tests {
  use http::uri::Authority;

  use crate::local::FileStorage;
  use htsget_test::util::default_dir;

  use super::*;

  #[test]
  fn data_url() {
    let result = FileStorage::<storage::file::File>::new(
      default_dir().join("data"),
      storage::file::File::default(),
    )
    .unwrap()
    .data_url(b"Hello World!".to_vec(), Some(Class::Header));
    let url = data_url::DataUrl::process(&result.url);
    let (result, _) = url.unwrap().decode_to_vec().unwrap();
    assert_eq!(result, b"Hello World!");
  }

  #[test]
  fn http_formatter_authority() {
    let formatter = storage::file::File::new(
      Scheme::Http,
      Authority::from_static("127.0.0.1:8080"),
      "data".to_string(),
    );
    test_formatter_authority(formatter, "http");
  }

  #[test]
  fn https_formatter_authority() {
    let formatter = storage::file::File::new(
      Scheme::Https,
      Authority::from_static("127.0.0.1:8080"),
      "data".to_string(),
    );
    test_formatter_authority(formatter, "https");
  }

  fn test_formatter_authority(formatter: storage::file::File, scheme: &str) {
    assert_eq!(
      formatter.format_url("path").unwrap(),
      format!("{}://127.0.0.1:8080/path", scheme)
    )
  }
}
