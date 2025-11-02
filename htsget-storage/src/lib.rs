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
use crate::local::FileStorage;
#[cfg(feature = "aws")]
use crate::s3::S3Storage;
use crate::types::{BytesPositionOptions, DataBlock, GetOptions, HeadOptions, RangeUrlOptions};
#[cfg(feature = "url")]
use crate::url::UrlStorage;
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose;
use cfg_if::cfg_if;
#[cfg(feature = "experimental")]
use htsget_config::encryption_scheme::EncryptionScheme;
use htsget_config::storage;
#[cfg(feature = "experimental")]
use htsget_config::storage::c4gh::C4GHKeys;
use pin_project_lite::pin_project;
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

#[cfg(feature = "experimental")]
pub mod c4gh;
pub mod error;
pub mod local;
#[cfg(feature = "aws")]
pub mod s3;
pub mod types;
#[cfg(feature = "url")]
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
  pub async fn from_c4gh_keys(
    keys: Option<&C4GHKeys>,
    encryption_scheme: Option<EncryptionScheme>,
    storage: Storage,
    query: &Query,
  ) -> Result<Storage> {
    match (keys, encryption_scheme) {
      (Some(keys), Some(EncryptionScheme::C4GH)) => {
        let (mut c4gh_keys, using_header) = keys
          .clone()
          .into_inner()
          .await
          .map_err(|err| StorageError::InternalError(err.to_string()))?;

        if let Some(using_header) = using_header {
          let public_key = using_header.get_public_key(query.request().headers())?;
          c4gh_keys
            .iter_mut()
            .for_each(|key| key.recipient_pubkey = public_key.clone());
        }

        Ok(Storage::new(C4GHStorage::new_box(
          c4gh_keys,
          storage.into_inner(),
        )))
      }
      (None, Some(EncryptionScheme::C4GH)) => Err(StorageError::UnsupportedFormat(
        "C4GH keys have not been configured for this id".to_string(),
      )),
      _ => Ok(storage),
    }
  }

  /// Create from local storage config.
  pub async fn from_file(file: &storage::file::File, _query: &Query) -> Result<Storage> {
    let storage = Storage::new(FileStorage::new(
      file.local_path(),
      file.clone(),
      file.ticket_headers().to_vec(),
    )?);

    cfg_if! {
      if #[cfg(feature = "experimental")] {
        Self::from_c4gh_keys(file.keys(), _query.encryption_scheme(), storage, _query).await
      } else {
        Ok(storage)
      }
    }
  }

  /// Create from s3 config.
  #[cfg(feature = "aws")]
  pub async fn from_s3(s3: &storage::s3::S3, _query: &Query) -> Result<Storage> {
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
        Self::from_c4gh_keys(s3.keys(), _query.encryption_scheme(), storage, _query).await
      } else {
        Ok(storage)
      }
    }
  }

  /// Create from url config.
  #[cfg(feature = "url")]
  pub async fn from_url(mut url: storage::url::Url, _query: &Query) -> Result<Storage> {
    let storage = Storage::new(UrlStorage::new(
      url
        .client_cloned()
        .map_err(|err| StorageError::InternalError(err.to_string()))?,
      url.url().clone(),
      url.response_url().clone(),
      url.forward_headers(),
      url.header_blacklist().to_vec(),
    ));

    cfg_if! {
      if #[cfg(feature = "experimental")] {
        Self::from_c4gh_keys(url.keys(), _query.encryption_scheme(), storage, _query).await
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
    let mut url = if let Some(origin) = self.ticket_origin() {
      origin.to_string()
    } else {
      format!("{}://{}", self.scheme(), self.authority())
    };
    if !url.ends_with('/') {
      url = format!("{url}/");
    }

    let url = ::url::Url::parse(&url).map_err(|err| StorageError::InvalidUri(err.to_string()))?;
    url
      .join(key.as_ref())
      .map_err(|err| StorageError::InvalidUri(err.to_string()))
      .map(|url| url.to_string())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::local::FileStorage;
  use htsget_config::config::advanced::CONTEXT_HEADER_PREFIX;
  use htsget_config::storage::c4gh::header::C4GHHeader;
  use htsget_config::types::{Request, Scheme};
  use htsget_test::util::{default_dir, default_dir_data};
  use http::uri::Authority;
  use http::{HeaderMap, HeaderName};
  use tokio::fs;

  #[test]
  fn data_url() {
    let result = FileStorage::<storage::file::File>::new(
      default_dir_data(),
      storage::file::File::default(),
      vec![],
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

  #[cfg(feature = "experimental")]
  #[tokio::test]
  async fn from_c4gh_keys() {
    let keys = tokio::spawn(async { Ok(C4GHKeys::from_key_pair(vec![], vec![])) });
    let storage = Storage::new(
      FileStorage::new(default_dir_data(), storage::file::File::default(), vec![]).unwrap(),
    );

    let result = Storage::from_c4gh_keys(
      Some(&C4GHKeys::from_join_handle(keys, None)),
      Some(EncryptionScheme::C4GH),
      storage.clone(),
      &Default::default(),
    )
    .await;
    assert!(result.is_ok());

    let result = Storage::from_c4gh_keys(None, None, storage.clone(), &Default::default()).await;
    assert!(result.is_ok());

    let public_key = fs::read_to_string(default_dir().join("data/c4gh/keys/alice.pub"))
      .await
      .unwrap();
    let encoded_key = general_purpose::STANDARD.encode(public_key);

    let mut headers = HeaderMap::new();
    headers.insert(
      format!("{CONTEXT_HEADER_PREFIX}Public-Key")
        .parse::<HeaderName>()
        .unwrap(),
      encoded_key.parse().unwrap(),
    );
    let query = Query::new(
      "id".to_string(),
      Format::Bam,
      Request::new("id".to_string(), Default::default(), headers),
    );
    let keys = tokio::spawn(async { Ok(C4GHKeys::from_key_pair(vec![], vec![])) });
    let result = Storage::from_c4gh_keys(
      Some(&C4GHKeys::from_join_handle(keys, Some(C4GHHeader))),
      Some(EncryptionScheme::C4GH),
      storage.clone(),
      &query,
    )
    .await;
    assert!(result.is_ok());

    let result = Storage::from_c4gh_keys(None, None, storage.clone(), &Default::default()).await;
    assert!(result.is_ok());

    let result = Storage::from_c4gh_keys(
      None,
      Some(EncryptionScheme::C4GH),
      storage,
      &Default::default(),
    )
    .await;
    assert!(matches!(result, Err(StorageError::UnsupportedFormat(_))));
  }

  fn test_formatter_authority(formatter: storage::file::File, scheme: &str) {
    assert_eq!(
      formatter.format_url("path").unwrap(),
      format!("{scheme}://127.0.0.1:8080/path")
    )
  }
}
