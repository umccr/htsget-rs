//! Module providing an implementation for the [Storage] trait using the local file system.
//!

use std::fmt::Debug;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs::File;
use tracing::debug;
use tracing::instrument;

use htsget_config::regex_resolver::RegexResolver;

use crate::htsget::Url;
use crate::storage::{resolve_id, Storage, UrlFormatter};

use super::{GetOptions, RangeUrlOptions, Result, StorageError};

/// Implementation for the [Storage] trait using the local file system. [T] is the type of the
/// server struct, which is used for formatting urls.
#[derive(Debug, Clone)]
pub struct LocalStorage<T> {
  base_path: PathBuf,
  id_resolver: RegexResolver,
  url_formatter: T,
}

impl<T: UrlFormatter + Send + Sync> LocalStorage<T> {
  pub fn new<P: AsRef<Path>>(
    base_path: P,
    id_resolver: RegexResolver,
    url_formatter: T,
  ) -> Result<Self> {
    base_path
      .as_ref()
      .to_path_buf()
      .canonicalize()
      .map_err(|_| StorageError::KeyNotFound(base_path.as_ref().to_string_lossy().to_string()))
      .map(|canonicalized_base_path| Self {
        base_path: canonicalized_base_path,
        id_resolver,
        url_formatter,
      })
  }

  pub fn base_path(&self) -> &Path {
    self.base_path.as_path()
  }

  pub(crate) fn get_path_from_key<K: AsRef<str>>(&self, key: K) -> Result<PathBuf> {
    let key: &str = key.as_ref();
    self
      .base_path
      .join(resolve_id(&self.id_resolver, &key)?)
      .canonicalize()
      .map_err(|_| StorageError::InvalidKey(key.to_string()))
      .and_then(|path| {
        path
          .starts_with(&self.base_path)
          .then_some(path)
          .ok_or_else(|| StorageError::InvalidKey(key.to_string()))
      })
      .and_then(|path| {
        path
          .is_file()
          .then_some(path)
          .ok_or_else(|| StorageError::KeyNotFound(key.to_string()))
      })
  }

  async fn get<K: AsRef<str>>(&self, key: K) -> Result<File> {
    let path = self.get_path_from_key(&key)?;
    File::open(path)
      .await
      .map_err(|_| StorageError::KeyNotFound(key.as_ref().to_string()))
  }
}

#[async_trait]
impl<T: UrlFormatter + Send + Sync + Debug> Storage for LocalStorage<T> {
  type Streamable = File;

  /// Get the file at the location of the key.
  #[instrument(level = "debug", skip(self))]
  async fn get<K: AsRef<str> + Send + Debug>(&self, key: K, _options: GetOptions) -> Result<File> {
    debug!(calling_from = ?self, key = key.as_ref(), "getting file with key {:?}", key.as_ref());
    self.get(key).await
  }

  /// Get a url for the file at key.
  #[instrument(level = "debug", skip(self))]
  async fn range_url<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: RangeUrlOptions,
  ) -> Result<Url> {
    let path = self.get_path_from_key(&key)?;
    let path = path
      .strip_prefix(&self.base_path)
      .map_err(|err| StorageError::InternalError(err.to_string()))?
      .to_string_lossy();

    let url = Url::new(self.url_formatter.format_url(&path)?);
    let url = options.apply(url);

    debug!(calling_from = ?self, key = key.as_ref(), ?url, "getting url with key {:?}", key.as_ref());
    Ok(url)
  }

  /// Get the size of the file.
  #[instrument(level = "debug", skip(self))]
  async fn head<K: AsRef<str> + Send + Debug>(&self, key: K) -> Result<u64> {
    let path = self.get_path_from_key(&key)?;
    let len = tokio::fs::metadata(path)
      .await
      .map_err(|err| StorageError::KeyNotFound(err.to_string()))?
      .len();

    debug!(calling_from = ?self, key = key.as_ref(), len, "size of key {:?} is {}", key.as_ref(), len);
    Ok(len)
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::future::Future;
  use std::matches;

  use tempfile::TempDir;
  use tokio::fs::{create_dir, File};
  use tokio::io::AsyncWriteExt;

  use crate::htsget::{Headers, Url};
  use crate::storage::data_server::HttpTicketFormatter;
  use crate::storage::{BytesPosition, GetOptions, RangeUrlOptions, StorageError};

  use super::*;

  #[tokio::test]
  async fn get_non_existing_key() {
    with_local_storage(|storage| async move {
      let result = storage.get("non-existing-key").await;
      assert!(matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "non-existing-key"));
    })
    .await;
  }

  #[tokio::test]
  async fn get_folder() {
    with_local_storage(|storage| async move {
      let result = Storage::get(&storage, "folder", GetOptions::default()).await;
      assert!(matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "folder"));
    })
    .await;
  }

  #[tokio::test]
  async fn get_forbidden_path() {
    with_local_storage(|storage| async move {
      let result = Storage::get(&storage, "folder/../../passwords", GetOptions::default()).await;
      assert!(
        matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "folder/../../passwords")
      );
    })
    .await;
  }

  #[tokio::test]
  async fn get_existing_key() {
    with_local_storage(|storage| async move {
      let result = Storage::get(&storage, "folder/../key1", GetOptions::default()).await;
      assert!(matches!(result, Ok(_)));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_non_existing_key() {
    with_local_storage(|storage| async move {
      let result =
        Storage::range_url(&storage, "non-existing-key", RangeUrlOptions::default()).await;
      assert!(matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "non-existing-key"));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_folder() {
    with_local_storage(|storage| async move {
      let result = Storage::range_url(&storage, "folder", RangeUrlOptions::default()).await;
      assert!(matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "folder"));
    })
    .await;
  }

  #[tokio::test]
  async fn url_with_forbidden_path() {
    with_local_storage(|storage| async move {
      let result = Storage::range_url(
        &storage,
        "folder/../../passwords",
        RangeUrlOptions::default(),
      )
      .await;
      assert!(
        matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "folder/../../passwords")
      );
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_existing_key() {
    with_local_storage(|storage| async move {
      let result = Storage::range_url(&storage, "folder/../key1", RangeUrlOptions::default()).await;
      let expected = Url::new("http://127.0.0.1:8081/data/key1");
      assert!(matches!(result, Ok(url) if url == expected));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_existing_key_with_specified_range() {
    with_local_storage(|storage| async move {
      let result = Storage::range_url(
        &storage,
        "folder/../key1",
        RangeUrlOptions::default().with_range(BytesPosition::new(Some(7), Some(10), None)),
      )
      .await;
      let expected = Url::new("http://127.0.0.1:8081/data/key1")
        .with_headers(Headers::default().with_header("Range", "bytes=7-9"));
      assert!(matches!(result, Ok(url) if url == expected));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_existing_key_with_specified_open_ended_range() {
    with_local_storage(|storage| async move {
      let result = Storage::range_url(
        &storage,
        "folder/../key1",
        RangeUrlOptions::default().with_range(BytesPosition::new(Some(7), None, None)),
      )
      .await;
      let expected = Url::new("http://127.0.0.1:8081/data/key1")
        .with_headers(Headers::default().with_header("Range", "bytes=7-"));
      assert!(matches!(result, Ok(url) if url == expected));
    })
    .await;
  }

  #[tokio::test]
  async fn file_size() {
    with_local_storage(|storage| async move {
      let result = Storage::head(&storage, "folder/../key1").await;
      let expected: u64 = 6;
      assert!(matches!(result, Ok(size) if size == expected));
    })
    .await;
  }

  pub(crate) fn create_base_path() -> TempDir {
    TempDir::new().unwrap()
  }

  pub(crate) async fn create_local_test_files() -> (String, TempDir) {
    let base_path = create_base_path();

    let folder_name = "folder";
    let key1 = "key1";
    let value1 = b"value1";
    let key2 = "key2";
    let value2 = b"value2";
    File::create(base_path.path().join(key1))
      .await
      .unwrap()
      .write_all(value1)
      .await
      .unwrap();
    create_dir(base_path.path().join(folder_name))
      .await
      .unwrap();
    File::create(base_path.path().join(folder_name).join(key2))
      .await
      .unwrap()
      .write_all(value2)
      .await
      .unwrap();

    (folder_name.to_string(), base_path)
  }

  async fn with_local_storage<F, Fut>(test: F)
  where
    F: FnOnce(LocalStorage<HttpTicketFormatter>) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    test(
      LocalStorage::new(
        base_path.path(),
        RegexResolver::new(".*", "$0").unwrap(),
        HttpTicketFormatter::new("127.0.0.1:8081".parse().unwrap(), "".to_string(), false),
      )
      .unwrap(),
    )
    .await
  }
}
