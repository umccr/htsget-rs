//! Module providing an implementation for the [Storage] trait using the local file system.
//!

use async_trait::async_trait;
use tokio::fs::File;

use crate::htsget::{Format, Url};
use crate::storage;
use crate::storage::async_storage::AsyncStorage;
use crate::storage::blocking::local::LocalStorage;
use crate::storage::key_extractor::KeyExtractor;

use super::{GetOptions, Result, StorageError, UrlOptions};

#[async_trait]
impl<K> AsyncStorage<K> for LocalStorage
where K: AsRef<str> + Send
{
  type Streamable = File;

  async fn get(&self, key: K, _options: GetOptions) -> Result<File> {
    let path = self.get_path_from_key(&key)?;
    File::open(path)
      .await
      .map_err(|e| StorageError::IoError(e, key.as_ref().to_string()))
  }

  async fn url(&self, key: K, options: UrlOptions) -> Result<Url> {
    storage::blocking::Storage::url(self, key, options)
  }

  async fn head(&self, key: K) -> Result<u64> {
    let key: &str = key.as_ref();
    let path = self.get_path_from_key(key)?;
    Ok(
      tokio::fs::metadata(path)
        .await
        .map_err(|err| StorageError::KeyNotFound(err.to_string()))?
        .len(),
    )
  }
}

struct SimpleKeyExtractor;

impl<K> KeyExtractor<K> for SimpleKeyExtractor
  where K: AsRef<str> + Send
{
  fn get_index_key<T: AsRef<str>>(&self, id: T, format: &Format) -> Result<K> {
    match format {
      Format::Bam => Ok(id + ".bam.bai"),
      Format::Cram => Ok(id + ".cram.crai"),
      Format::Vcf => Ok(id + ".vcf.gz.tbi"),
      Format::Bcf => Ok(id + ".bcf.csi"),
      Format::Unsupported(format) => Err(StorageError::InvalidFormat(format))
    }
  }

  fn get_file_key<T: AsRef<str>>(&self, id: T, format: &Format) -> Result<K> {
    match format {
      Format::Bam => Ok(id + ".bam"),
      Format::Cram => Ok(id + ".cram"),
      Format::Vcf => Ok(id + ".vcf.gz"),
      Format::Bcf => Ok(id + ".bcf"),
      Format::Unsupported(format) => Err(StorageError::InvalidFormat(format))
    }
  }
}

#[cfg(test)]
mod tests {
  use std::future::Future;
  use std::matches;

  use tokio::fs::{create_dir, File};
  use tokio::io::AsyncWriteExt;

  use htsget_id_resolver::RegexResolver;

  use crate::htsget::{Headers, Url};
  use crate::storage::{BytesRange, GetOptions, StorageError, UrlOptions};

  use super::*;

  #[tokio::test]
  async fn get_non_existing_key() {
    with_local_storage(|storage| async move {
      let result = AsyncStorage::get(&storage, "non-existing-key", GetOptions::default()).await;
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
    test(LocalStorage::new(base_path.path(), RegexResolver::new(".*", "$0").unwrap()).unwrap())
      .await
  }
}
