//! Module providing an implementation for the [Storage] trait using the local file system.
//!

use std::fmt::Debug;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs::File;
use tracing::debug;
use tracing::instrument;

use htsget_config::Query;

use crate::htsget::Url;
use crate::storage::{resolve_id, Storage, UrlFormatter};
use crate::RegexResolver;

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

  pub(crate) fn get_path_from_key(&self, query: &Query) -> Result<PathBuf> {
    self
      .base_path
      .join(resolve_id(&self.id_resolver, query)?)
      .canonicalize()
      .map_err(|_| StorageError::InvalidKey(query.id().to_string()))
      .and_then(|path| {
        path
          .starts_with(&self.base_path)
          .then_some(path)
          .ok_or_else(|| StorageError::InvalidKey(query.id().to_string()))
      })
      .and_then(|path| {
        path
          .is_file()
          .then_some(path)
          .ok_or_else(|| StorageError::KeyNotFound(query.id().to_string()))
      })
  }

  pub async fn get(&self, query: &Query) -> Result<File> {
    let path = self.get_path_from_key(query)?;
    File::open(path)
      .await
      .map_err(|_| StorageError::KeyNotFound(query.id().to_string()))
  }
}

#[async_trait]
impl<T: UrlFormatter + Send + Sync + Debug> Storage for LocalStorage<T> {
  type Streamable = File;

  /// Get the file at the location of the key.
  #[instrument(level = "debug", skip(self))]
  async fn get(&self, query: &Query, _options: GetOptions) -> Result<File> {
    debug!(calling_from = ?self, id = query.id(), "getting file with key {:?}", query.id());
    self.get(query).await
  }

  /// Get a url for the file at key.
  #[instrument(level = "debug", skip(self))]
  async fn range_url(&self, query: &Query, options: RangeUrlOptions) -> Result<Url> {
    let path = self.get_path_from_key(query)?;
    let path = path
      .strip_prefix(&self.base_path)
      .map_err(|err| StorageError::InternalError(err.to_string()))?
      .to_string_lossy();

    let url = Url::new(self.url_formatter.format_url(&path)?);
    let url = options.apply(url);

    debug!(calling_from = ?self, id = query.id(), ?url, "getting url with key {:?}", query.id());
    Ok(url)
  }

  /// Get the size of the file.
  #[instrument(level = "debug", skip(self))]
  async fn head(&self, query: &Query) -> Result<u64> {
    let path = self.get_path_from_key(query)?;
    let len = tokio::fs::metadata(path)
      .await
      .map_err(|err| StorageError::KeyNotFound(err.to_string()))?
      .len();

    debug!(calling_from = ?self, id = query.id(), len, "size of key {:?} is {}", query.id(), len);
    Ok(len)
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::future::Future;
  use std::matches;

  use htsget_config::config::cors::CorsConfig;
  use tempfile::TempDir;
  use tokio::fs::{create_dir, File};
  use tokio::io::AsyncWriteExt;

  use htsget_config::regex_resolver::StorageType;
  use htsget_config::Format::Bam;

  use crate::htsget::{Headers, Url};
  use crate::storage::data_server::HttpTicketFormatter;
  use crate::storage::{BytesPosition, GetOptions, RangeUrlOptions, StorageError};

  use super::*;

  #[tokio::test]
  async fn get_non_existing_key() {
    with_local_storage(|storage| async move {
      let result = storage.get(&Query::new("non-existing-key", Bam)).await;
      assert!(matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "non-existing-key"));
    })
    .await;
  }

  #[tokio::test]
  async fn get_folder() {
    with_local_storage(|storage| async move {
      let result = Storage::get(&storage, &Query::new("folder", Bam), GetOptions::default()).await;
      assert!(matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "folder"));
    })
    .await;
  }

  #[tokio::test]
  async fn get_forbidden_path() {
    with_local_storage(|storage| async move {
      let result = Storage::get(
        &storage,
        &Query::new("folder/../../passwords", Bam),
        GetOptions::default(),
      )
      .await;
      assert!(
        matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "folder/../../passwords")
      );
    })
    .await;
  }

  #[tokio::test]
  async fn get_existing_key() {
    with_local_storage(|storage| async move {
      let result = Storage::get(
        &storage,
        &Query::new("folder/../key1", Bam),
        GetOptions::default(),
      )
      .await;
      assert!(matches!(result, Ok(_)));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_non_existing_key() {
    with_local_storage(|storage| async move {
      let result = Storage::range_url(
        &storage,
        &Query::new("non-existing-key", Bam),
        RangeUrlOptions::default(),
      )
      .await;
      assert!(matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "non-existing-key"));
    })
    .await;
  }

  #[tokio::test]
  async fn url_of_folder() {
    with_local_storage(|storage| async move {
      let result = Storage::range_url(
        &storage,
        &Query::new("folder", Bam),
        RangeUrlOptions::default(),
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
        &Query::new("folder/../../passwords", Bam),
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
      let result = Storage::range_url(
        &storage,
        &Query::new("folder/../key1", Bam),
        RangeUrlOptions::default(),
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
        &Query::new("folder/../key1", Bam),
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
        &Query::new("folder/../key1", Bam),
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
      let result = Storage::head(&storage, &Query::new("folder/../key1", Bam)).await;
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
    F: FnOnce(LocalStorage<HttpTicketFormatter>) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    test(
      LocalStorage::new(
        base_path.path(),
        RegexResolver::new(StorageType::default(), ".*", "$0", Default::default()).unwrap(),
        HttpTicketFormatter::new("127.0.0.1:8081".parse().unwrap(), CorsConfig::default()),
      )
      .unwrap(),
    )
    .await
  }
}
