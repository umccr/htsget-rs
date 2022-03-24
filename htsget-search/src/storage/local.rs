//! Module providing an implementation for the [Storage] trait using the local file system.
//!

use std::path::{Path, PathBuf};
use async_trait::async_trait;
use tokio::fs::File;
use htsget_id_resolver::{HtsGetIdResolver, RegexResolver};

use crate::htsget::{Format, Headers, Url};
use crate::storage::async_storage::AsyncStorage;

use super::{GetOptions, Result, StorageError, UrlOptions};

/// Implementation for the [Storage] trait using the local file system.
#[derive(Debug)]
pub struct LocalStorage {
  base_path: PathBuf,
  id_resolver: RegexResolver,
}

impl LocalStorage {
  pub fn new<P: AsRef<Path>>(base_path: P, id_resolver: RegexResolver) -> Result<Self> {
    base_path
      .as_ref()
      .to_path_buf()
      .canonicalize()
      .map_err(|_| StorageError::KeyNotFound(base_path.as_ref().to_string_lossy().to_string()))
      .map(|canonicalized_base_path| Self {
        base_path: canonicalized_base_path,
        id_resolver,
      })
  }

  pub fn base_path(&self) -> &Path {
    self.base_path.as_path()
  }

  pub(crate) fn get_path_from_key<K: AsRef<str>>(&self, key: K) -> Result<PathBuf> {
    let key: &str = key.as_ref();
    self
      .base_path
      .join(
        self
          .id_resolver
          .resolve_id(key)
          .ok_or_else(|| StorageError::InvalidKey(key.to_string()))?,
      )
      .canonicalize()
      .map_err(|_| StorageError::InvalidKey(key.to_string()))
      .and_then(|path| {
        path
          .starts_with(&self.base_path)
          .then(|| path)
          .ok_or_else(|| StorageError::InvalidKey(key.to_string()))
      })
      .and_then(|path| {
        path
          .is_file()
          .then(|| path)
          .ok_or_else(|| StorageError::KeyNotFound(key.to_string()))
      })
  }

  async fn get<K: AsRef<str>>(&self, key: K) -> Result<File> {
    let path = self.get_path_from_key(&key)?;
    File::open(path)
      .await
      .map_err(|e| StorageError::IoError(e, key.as_ref().to_string()))
  }
}

#[async_trait]
impl AsyncStorage for LocalStorage
{
  type Streamable = File;

  async fn get<K: AsRef<str> + Send>(&self, key: K, _options: GetOptions) -> Result<File> {
    self.get(key).await
  }

  async fn url<K: AsRef<str> + Send>(&self, key: K, options: UrlOptions) -> Result<Url> {
    let range_start = options
      .range
      .start
      .map(|start| start.to_string())
      .unwrap_or_else(|| "".to_string());
    let range_end = options
      .range
      .end
      .map(|end| end.to_string())
      .unwrap_or_else(|| "".to_string());

    // TODO file:// is not allowed by the spec. We should consider including an static http server for the base_path
    let path = self.get_path_from_key(&key)?;
    let url = Url::new(format!("file://{}", path.to_string_lossy()));
    let url = if range_start.is_empty() && range_end.is_empty() {
      url
    } else {
      url.with_headers(
        Headers::default().with_header("Range", format!("bytes={}-{}", range_start, range_end)),
      )
    };
    let url = url.with_class(options.class);
    Ok(url)
  }

  async fn head<K: AsRef<str> + Send>(&self, key: K) -> Result<u64> {
    let path = self.get_path_from_key(&key)?;
    Ok(
      tokio::fs::metadata(path)
        .await
        .map_err(|err| StorageError::KeyNotFound(err.to_string()))?
        .len(),
    )
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::future::Future;
  use std::matches;
  use tempfile::TempDir;

  use tokio::fs::{create_dir, File};
  use tokio::io::AsyncWriteExt;

  use htsget_id_resolver::RegexResolver;

  use crate::htsget::{Headers, Url};
  use crate::storage::{BytesRange, GetOptions, StorageError, UrlOptions};

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
      let result = AsyncStorage::get(&storage, "folder", GetOptions::default()).await;
      assert!(matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "folder"));
    })
    .await;
  }

  #[tokio::test]
  async fn get_forbidden_path() {
    with_local_storage(|storage| async move {
      let result =
        AsyncStorage::get(&storage, "folder/../../passwords", GetOptions::default()).await;
      assert!(
        matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "folder/../../passwords")
      );
    })
    .await;
  }

  #[tokio::test]
  async fn get_existing_key() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::get(&storage, "folder/../key1", GetOptions::default()).await;
      assert!(matches!(result, Ok(_)));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_non_existing_key() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::url(&storage, "non-existing-key", UrlOptions::default()).await;
      assert!(matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "non-existing-key"));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_folder() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::url(&storage, "folder", UrlOptions::default()).await;
      assert!(matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "folder"));
    })
    .await;
  }

  #[tokio::test]
  async fn url_with_forbidden_path() {
    with_local_storage(|storage| async move {
      let result =
        AsyncStorage::url(&storage, "folder/../../passwords", UrlOptions::default()).await;
      assert!(
        matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "folder/../../passwords")
      );
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_existing_key() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::url(&storage, "folder/../key1", UrlOptions::default()).await;
      let expected = Url::new(format!(
        "file://{}",
        storage.base_path().join("key1").to_string_lossy()
      ));
      assert!(matches!(result, Ok(url) if url == expected));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_existing_key_with_specified_range() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::url(
        &storage,
        "folder/../key1",
        UrlOptions::default().with_range(BytesRange::new(Some(7), Some(9))),
      )
      .await;
      let expected = Url::new(format!(
        "file://{}",
        storage.base_path().join("key1").to_string_lossy()
      ))
      .with_headers(Headers::default().with_header("Range", "bytes=7-9"));
      assert!(matches!(result, Ok(url) if url == expected));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_existing_key_with_specified_open_ended_range() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::url(
        &storage,
        "folder/../key1",
        UrlOptions::default().with_range(BytesRange::new(Some(7), None)),
      )
      .await;
      let expected = Url::new(format!(
        "file://{}",
        storage.base_path().join("key1").to_string_lossy()
      ))
      .with_headers(Headers::default().with_header("Range", "bytes=7-"));
      assert!(matches!(result, Ok(url) if url == expected));
    })
    .await;
  }

  #[tokio::test]
  async fn file_size() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::head(&storage, "folder/../key1").await;
      let expected: u64 = 6;
      assert!(matches!(result, Ok(size) if size == expected));
    })
    .await;
  }

  pub(crate) async fn create_local_test_files() -> (PathBuf, TempDir) {
    let base_path = tempfile::TempDir::new().unwrap();

    File::create(base_path.path().join("key1"))
      .await
      .unwrap()
      .write_all(b"value1")
      .await
      .unwrap();
    create_dir(base_path.path().join("folder")).await.unwrap();
    File::create(base_path.path().join("folder").join("key2"))
      .await
      .unwrap()
      .write_all(b"value2")
      .await
      .unwrap();

    (base_path.path().to_path_buf().join("folder"), base_path)
  }

  async fn with_local_storage<F, Fut>(test: F)
  where
    F: FnOnce(LocalStorage) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    test(LocalStorage::new(base_path.path(), RegexResolver::new(".*", "$0").unwrap()).unwrap())
      .await
  }
}
