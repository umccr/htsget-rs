//! Module providing an implementation for the [Storage] trait using the local file system.
//!

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs::File;

use htsget_config::regex_resolver::{HtsGetIdResolver, RegexResolver};

use crate::htsget::Url;
use crate::storage::{Storage, UrlFormatter};

use super::{GetOptions, Result, StorageError, UrlOptions};

/// Implementation for the [Storage] trait using the local file system. [T] is the type of the
/// server struct, which is used for formatting urls.
#[derive(Debug)]
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
    Ok(File::open(path).await?)
  }
}

#[async_trait]
impl<T: UrlFormatter + Send + Sync> Storage for LocalStorage<T> {
  type Streamable = File;

  /// Get the file at the location of the key.
  async fn get<K: AsRef<str> + Send>(&self, key: K, _options: GetOptions) -> Result<File> {
    self.get(key).await
  }

  /// Get a url for the file at key.
  async fn url<K: AsRef<str> + Send>(&self, key: K, options: UrlOptions) -> Result<Url> {
    let path = self.get_path_from_key(&key)?;
    let url = Url::new(
      self
        .url_formatter
        .format_url(path.to_string_lossy().to_string())?,
    );
    Ok(options.apply(url))
  }

  /// Get the size of the file.
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

  use crate::htsget::{Headers, Url};
  use crate::storage::{BytesRange, GetOptions, StorageError, UrlOptions};
  use crate::storage::axum_server::HttpsFormatter;

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
      let result = Storage::url(&storage, "non-existing-key", UrlOptions::default()).await;
      assert!(matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "non-existing-key"));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_folder() {
    with_local_storage(|storage| async move {
      let result = Storage::url(&storage, "folder", UrlOptions::default()).await;
      assert!(matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "folder"));
    })
    .await;
  }

  #[tokio::test]
  async fn url_with_forbidden_path() {
    with_local_storage(|storage| async move {
      let result = Storage::url(&storage, "folder/../../passwords", UrlOptions::default()).await;
      assert!(
        matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "folder/../../passwords")
      );
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_existing_key() {
    with_local_storage(|storage| async move {
      let result = Storage::url(&storage, "folder/../key1", UrlOptions::default()).await;
      let expected = Url::new(format!(
        "https://127.0.0.1:8081{}",
        storage.base_path().join("key1").to_string_lossy()
      ));
      assert!(matches!(result, Ok(url) if url == expected));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_existing_key_with_specified_range() {
    with_local_storage(|storage| async move {
      let result = Storage::url(
        &storage,
        "folder/../key1",
        UrlOptions::default().with_range(BytesRange::new(Some(7), Some(9))),
      )
      .await;
      let expected = Url::new(format!(
        "https://127.0.0.1:8081{}",
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
      let result = Storage::url(
        &storage,
        "folder/../key1",
        UrlOptions::default().with_range(BytesRange::new(Some(7), None)),
      )
      .await;
      let expected = Url::new(format!(
        "https://127.0.0.1:8081{}",
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
      let result = Storage::head(&storage, "folder/../key1").await;
      let expected: u64 = 6;
      assert!(matches!(result, Ok(size) if size == expected));
    })
    .await;
  }

  pub(crate) async fn create_local_test_files() -> (String, TempDir) {
    let base_path = tempfile::TempDir::new().unwrap();

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
    F: FnOnce(LocalStorage<HttpsFormatter>) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    test(
      LocalStorage::new(
        base_path.path(),
        RegexResolver::new(".*", "$0").unwrap(),
        HttpsFormatter::new("127.0.0.1", "8081").unwrap(),
      )
      .unwrap(),
    )
    .await
  }
}
