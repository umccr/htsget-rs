//! Module providing an implementation for the [Storage] trait using the local file system.
//!

use std::path::PathBuf;

use async_trait::async_trait;

use crate::htsget::Url;
use crate::storage;
use crate::storage::async_storage::AsyncStorage;
use crate::storage::blocking::local::LocalStorage;

/// Implementation for the [Storage] trait using the local file system.
// #[derive(Debug)]
// pub struct LocalStorage {
//   base_path: PathBuf,
//   address: String,
// }

// impl LocalStorage {
//   pub fn new<P: AsRef<Path>>(base_path: P, address: impl Into<String>) -> Result<Self> {
//     base_path
//       .as_ref()
//       .to_path_buf()
//       .canonicalize()
//       .map_err(|_| StorageError::NotFound(base_path.as_ref().to_string_lossy().to_string()))
//       .map(|canonicalized_base_path| Self {
//         base_path: canonicalized_base_path,
//         address: address.into(),
//       })
//   }

//   pub fn base_path(&self) -> &Path {
//     self.base_path.as_path()
//   }

//   fn get_path_from_key<K: AsRef<str>>(&self, key: K) -> Result<PathBuf> {
//     let key: &str = key.as_ref();
//     self
//       .base_path
//       .join(key)
//       .canonicalize()
//       .map_err(|_| StorageError::InvalidKey(key.to_string()))
//       .and_then(|path| {
//         path
//           .starts_with(&self.base_path)
//           .then(|| path)
//           .ok_or_else(|| StorageError::InvalidKey(key.to_string()))
//       })
//       .and_then(|path| {
//         path
//           .is_file()
//           .then(|| path)
//           .ok_or_else(|| StorageError::NotFound(key.to_string()))
//       })
//   }
// }

#[async_trait]
impl AsyncStorage for LocalStorage {
  async fn get<K: AsRef<str> + Send>(&self, key: K, _options: GetOptions) -> Result<PathBuf> {
    self.get_path_from_key(key)
  }

  async fn url<K: AsRef<str> + Send>(&self, key: K, options: UrlOptions) -> Result<Url> {
    storage::blocking::Storage::url(self, key, options) {
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

    let captured_key = key.as_ref().to_string();
    let path = self
      .get_path_from_key(key)?
      .strip_prefix(&self.base_path)
      .map_err(|_| StorageError::NotFound(captured_key))?
      .to_owned();
    let url = Url::new(format!(
      "http://{}/{}",
      self.address,
      path.to_string_lossy()
    ));
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
    let key: &str = key.as_ref();
    let path = self.get_path_from_key(key)?;
    Ok(
      tokio::fs::metadata(path)
        .await
        .map_err(|err| StorageError::NotFound(err.to_string()))?
        .len(),
    )
  }
}

#[cfg(test)]
mod tests {
  use std::future::Future;

  use tokio::fs::{create_dir, File};
  use tokio::io::AsyncWriteExt;

  use crate::htsget::{Headers, Url};
  use crate::storage::blocking::local::LocalStorage;
  use crate::storage::{BytesRange, GetOptions, StorageError, UrlOptions};
  use htsget_id_resolver::RegexResolver;

  use super::*;

  #[tokio::test]
  async fn get_non_existing_key() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::get(&storage, "non-existing-key", GetOptions::default())
        .await
        .map(|path| path.to_string_lossy().to_string());
      assert_eq!(
        result,
        Err(StorageError::InvalidKey("non-existing-key".to_string()))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn get_folder() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::get(&storage, "folder", GetOptions::default())
        .await
        .map(|path| path.to_string_lossy().to_string());
      assert_eq!(result, Err(StorageError::NotFound("folder".to_string())));
    })
    .await;
  }

  #[tokio::test]
  async fn get_forbidden_path() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::get(&storage, "folder/../../passwords", GetOptions::default())
        .await
        .map(|path| path.to_string_lossy().to_string());
      assert_eq!(
        result,
        Err(StorageError::InvalidKey(
          "folder/../../passwords".to_string()
        ))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn get_existing_key() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::get(&storage, "folder/../key1", GetOptions::default())
        .await
        .map(|path| path.to_string_lossy().to_string());
      assert_eq!(
        result,
        Ok(format!(
          "{}",
          storage.base_path().join("key1").to_string_lossy()
        ))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_non_existing_key() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::url(&storage, "non-existing-key", UrlOptions::default()).await;
      assert_eq!(
        result,
        Err(StorageError::InvalidKey("non-existing-key".to_string()))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_folder() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::url(&storage, "folder", UrlOptions::default()).await;
      assert_eq!(result, Err(StorageError::NotFound("folder".to_string())));
    })
    .await;
  }

  #[tokio::test]
  async fn url_with_forbidden_path() {
    with_local_storage(|storage| async move {
      let result =
        AsyncStorage::url(&storage, "folder/../../passwords", UrlOptions::default()).await;
      assert_eq!(
        result,
        Err(StorageError::InvalidKey(
          "folder/../../passwords".to_string()
        ))
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
      assert_eq!(result, Ok(expected));
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
      assert_eq!(
        result,
        Ok(
          Url::new(format!("http://localhost/data/key1",))
            .with_headers(Headers::default().with_header("Range", "bytes=7-9"))
        )
      );
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
      assert_eq!(
        result,
        Ok(
          Url::new(format!("http://localhost/data/key1",))
            .with_headers(Headers::default().with_header("Range", "bytes=7-"))
        )
      );
    })
    .await;
  }

  #[tokio::test]
  async fn file_size() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::head(&storage, "folder/../key1").await;
      let expected: u64 = 6;
      assert_eq!(result, Ok(expected));
    })
    .await;
  }

  async fn with_local_storage<F, Fut>(test: F)
  where
    F: FnOnce(LocalStorage) -> Fut,
    Fut: Future<Output = ()>,
  {
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
//    test(LocalStorage::new(base_path.path(), "localhost/data").unwrap())
    test(LocalStorage::new(base_path.path(), RegexResolver::new(".*", "$0").unwrap()).unwrap())
      .await
  }
}
