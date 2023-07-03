//! Module providing an implementation for the [Storage] trait using the local file system.
//!

use std::fmt::Debug;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs::File;
use tracing::debug;
use tracing::instrument;

use crate::storage::{HeadOptions, Storage, UrlFormatter};
use crate::Url as HtsGetUrl;
use url::Url;
use super::crypt4gh::Crypt4gh;

use super::{GetOptions, RangeUrlOptions, Result, StorageError};

/// Implementation for the [Storage] trait using the local file system. [T] is the type of the
/// server struct, which is used for formatting urls.
#[derive(Debug, Clone)]
pub struct LocalStorage<T> {
  base_path: PathBuf,
  url_formatter: T,
  crypy4gh_data: Crypt4GhData,
}

#[derive(Debug, Clone)]
pub struct Crypt4GhData {
  
}

impl<T> Crypt4gh for LocalStorage<T> {
  type Streamable = File;
}

impl<T: UrlFormatter + Send + Sync> LocalStorage<T> {
  pub fn new<P: AsRef<Path>>(base_path: P, url_formatter: T) -> Result<Self> {
    base_path
      .as_ref()
      .to_path_buf()
      .canonicalize()
      .map_err(|_| StorageError::KeyNotFound(base_path.as_ref().to_string_lossy().to_string()))
      .map(|canonicalized_base_path| Self {
        base_path: canonicalized_base_path,
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
      .join(key)
      .canonicalize()
      .map_err(|err| {
        if let ErrorKind::NotFound = err.kind() {
          StorageError::KeyNotFound(key.to_string())
        } else {
          StorageError::InvalidKey(key.to_string())
        }
      })
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

  pub async fn get<K: AsRef<str>>(&self, key: K) -> Result<File> {
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
  async fn get<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    _options: GetOptions<'_>,
  ) -> Result<File> {
    debug!(calling_from = ?self, key = key.as_ref(), "getting file with key {:?}", key.as_ref());
    let file = self.get(key).await?;

    let decrypted_data = decrypt_wrapper(file);

    Ok(decryped_data)
  }


  // /// Decrypt Crypt4GH payload
  // async fn decrypt_crypt4gh_header(data: &[u8]) -> File {
  //   let decryped_data = self.decrypt_header(data, self.crypy4gh_data.keys, self.crypy4gh_data.public_keys);
  // }
  
  /// Get a url for the file at key.
  #[instrument(level = "debug", skip(self))]
  async fn range_url<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: RangeUrlOptions<'_>,
  ) -> Result<HtsGetUrl> {
    let path = self.get_path_from_key(&key)?;

    let base_url = Url::from_file_path(&self.base_path)
      .map_err(|_| StorageError::UrlParseError("failed to parse base path as url".to_string()))?;
    let path_url = Url::from_file_path(path)
      .map_err(|_| StorageError::UrlParseError("failed to parse key path as url".to_string()))?;

    // Get the difference between the two URLs and strip and leading slashes.
    let path = path_url
      .path()
      .strip_prefix(base_url.path())
      .ok_or_else(|| {
        StorageError::UrlParseError("failed parse relative component of key path url".to_string())
      })?;
    let path = path.trim_start_matches('/');

    let url = HtsGetUrl::new(self.url_formatter.format_url(path)?);
    let url = options.apply(url);

    debug!(calling_from = ?self, key = key.as_ref(), ?url, "getting url with key {:?}", key.as_ref());

    Ok(url)
  }

  /// Get the size of the file.
  #[instrument(level = "debug", skip(self))]
  async fn head<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    _options: HeadOptions<'_>,
  ) -> Result<u64> {
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

  use http::uri::Authority;
  use tempfile::TempDir;
  use tokio::fs::{create_dir, File};
  use tokio::io::AsyncWriteExt;

  use htsget_config::storage::local::LocalStorage as ConfigLocalStorage;
  use htsget_config::types::Scheme;

  use crate::storage::{BytesPosition, GetOptions, RangeUrlOptions, StorageError};
  use crate::{Headers, Url};

  use super::*;

  #[tokio::test]
  async fn get_non_existing_key() {
    with_local_storage(|storage| async move {
      let result = storage.get("non-existing-key").await;
      assert!(matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "non-existing-key"));
    })
    .await;
  }

  #[tokio::test]
  async fn get_folder() {
    with_local_storage(|storage| async move {
      let result = Storage::get(
        &storage,
        "folder",
        GetOptions::new_with_default_range(&Default::default()),
      )
      .await;
      assert!(matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "folder"));
    })
    .await;
  }

  #[tokio::test]
  async fn get_forbidden_path() {
    with_local_storage(|storage| async move {
      let result = Storage::get(
        &storage,
        "folder/../../passwords",
        GetOptions::new_with_default_range(&Default::default()),
      )
      .await;
      assert!(
        matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "folder/../../passwords")
      );
    })
    .await;
  }

  #[tokio::test]
  async fn get_existing_key() {
    with_local_storage(|storage| async move {
      let result = Storage::get(
        &storage,
        "folder/../key1",
        GetOptions::new_with_default_range(&Default::default()),
      )
      .await;
      assert!(result.is_ok());
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_non_existing_key() {
    with_local_storage(|storage| async move {
      let result = Storage::range_url(
        &storage,
        "non-existing-key",
        RangeUrlOptions::new_with_default_range(&Default::default()),
      )
      .await;
      assert!(matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "non-existing-key"));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_folder() {
    with_local_storage(|storage| async move {
      let result = Storage::range_url(
        &storage,
        "folder",
        RangeUrlOptions::new_with_default_range(&Default::default()),
      )
      .await;
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
        RangeUrlOptions::new_with_default_range(&Default::default()),
      )
      .await;
      assert!(
        matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "folder/../../passwords")
      );
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_existing_key() {
    with_local_storage(|storage| async move {
      let result = Storage::range_url(
        &storage,
        "folder/../key1",
        RangeUrlOptions::new_with_default_range(&Default::default()),
      )
      .await;
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
        RangeUrlOptions::new(
          BytesPosition::new(Some(7), Some(10), None),
          &Default::default(),
        ),
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
        RangeUrlOptions::new(BytesPosition::new(Some(7), None, None), &Default::default()),
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
      let result = Storage::head(
        &storage,
        "folder/../key1",
        HeadOptions::new(&Default::default()),
      )
      .await;
      let expected: u64 = 6;
      assert!(matches!(result, Ok(size) if size == expected));
    })
    .await;
  }

  pub(crate) async fn create_local_test_files() -> (String, TempDir) {
    let base_path = TempDir::new().unwrap();

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
    F: FnOnce(LocalStorage<ConfigLocalStorage>) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    test(
      LocalStorage::new(
        base_path.path(),
        ConfigLocalStorage::new(
          Scheme::Http,
          Authority::from_static("127.0.0.1:8081"),
          "data".to_string(),
          "/data".to_string(),
        ),
      )
      .unwrap(),
    )
    .await
  }
}
