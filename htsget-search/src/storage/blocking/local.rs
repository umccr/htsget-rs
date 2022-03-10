//! Module providing an implementation for the [Storage] trait using the local file system.
//!
use std::path::{Path, PathBuf};

use crate::htsget::{Headers, Url};

use super::{GetOptions, Result, Storage, StorageError, UrlOptions};
use htsget_id_resolver::{HtsGetIdResolver, RegexResolver};

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
}

impl Storage for LocalStorage {
  fn get<K: AsRef<str>>(&self, key: K, _options: GetOptions) -> Result<PathBuf> {
    self.get_path_from_key(key)
  }

  fn url<K: AsRef<str>>(&self, key: K, options: UrlOptions) -> Result<Url> {
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
    let path = self.get_path_from_key(key)?;
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

  fn head<K: AsRef<str>>(&self, key: K) -> Result<u64> {
    let key: &str = key.as_ref();
    let path = self.get_path_from_key(key)?;
    Ok(
      std::fs::metadata(path)
        .map_err(|err| StorageError::KeyNotFound(err.to_string()))?
        .len(),
    )
  }
}

#[cfg(test)]
mod tests {
  use std::fs::{create_dir, File};
  use std::io::prelude::*;

  use crate::htsget::{Headers, Url};
  use crate::storage::blocking::local::LocalStorage;
  use crate::storage::{BytesRange, GetOptions, StorageError, UrlOptions};

  use super::*;

  #[test]
  fn get_non_existing_key() {
    with_local_storage(|storage| {
      let result = storage
        .get("non-existing-key", GetOptions::default())
        .map(|path| path.to_string_lossy().to_string());
      assert!(matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "non-existing-key"));
    });
  }

  #[test]
  fn get_folder() {
    with_local_storage(|storage| {
      let result = storage
        .get("folder", GetOptions::default())
        .map(|path| path.to_string_lossy().to_string());
      assert!(matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "folder"));
    });
  }

  #[test]
  fn get_forbidden_path() {
    with_local_storage(|storage| {
      let result = storage
        .get("folder/../../passwords", GetOptions::default())
        .map(|path| path.to_string_lossy().to_string());
      assert!(
        matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "folder/../../passwords")
      );
    });
  }

  #[test]
  fn get_existing_key() {
    with_local_storage(|storage| {
      let result = storage
        .get("folder/../key1", GetOptions::default())
        .map(|path| path.to_string_lossy().to_string());
      let expected = format!("{}", storage.base_path().join("key1").to_string_lossy());
      assert!(matches!(result, Ok(msg) if msg == expected));
    });
  }

  #[test]
  fn url_of_non_existing_key() {
    with_local_storage(|storage| {
      let result = storage.url("non-existing-key", UrlOptions::default());
      assert!(matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "non-existing-key"));
    });
  }

  #[test]
  fn url_of_folder() {
    with_local_storage(|storage| {
      let result = storage.url("folder", UrlOptions::default());
      assert!(matches!(result, Err(StorageError::KeyNotFound(msg)) if msg == "folder"));
    });
  }

  #[test]
  fn url_with_forbidden_path() {
    with_local_storage(|storage| {
      let result = storage.url("folder/../../passwords", UrlOptions::default());
      assert!(
        matches!(result, Err(StorageError::InvalidKey(msg)) if msg == "folder/../../passwords")
      );
    });
  }

  #[test]
  fn url_of_existing_key() {
    with_local_storage(|storage| {
      let result = storage.url("folder/../key1", UrlOptions::default());
      let expected = Url::new(format!(
        "file://{}",
        storage.base_path().join("key1").to_string_lossy()
      ));
      assert!(matches!(result, Ok(url) if url == expected));
    });
  }

  #[test]
  fn url_of_existing_key_with_specified_range() {
    with_local_storage(|storage| {
      let result = storage.url(
        "folder/../key1",
        UrlOptions::default().with_range(BytesRange::new(Some(7), Some(9))),
      );
      let expected = Url::new(format!(
        "file://{}",
        storage.base_path().join("key1").to_string_lossy()
      ))
      .with_headers(Headers::default().with_header("Range", "bytes=7-9"));
      assert!(matches!(result, Ok(url) if url == expected));
    });
  }

  #[test]
  fn url_of_existing_key_with_specified_open_ended_range() {
    with_local_storage(|storage| {
      let result = storage.url(
        "folder/../key1",
        UrlOptions::default().with_range(BytesRange::new(Some(7), None)),
      );
      let expected = Url::new(format!(
        "file://{}",
        storage.base_path().join("key1").to_string_lossy()
      ))
      .with_headers(Headers::default().with_header("Range", "bytes=7-"));
      assert!(matches!(result, Ok(url) if url == expected));
    });
  }

  #[test]
  fn file_size() {
    with_local_storage(|storage| {
      let result = storage.head("folder/../key1");
      let expected: u64 = 6;
      assert!(matches!(result, Ok(size) if size == expected));
    });
  }

  fn with_local_storage(test: impl Fn(LocalStorage)) {
    let base_path = tempfile::TempDir::new().unwrap();
    File::create(base_path.path().join("key1"))
      .unwrap()
      .write_all(b"value1")
      .unwrap();
    create_dir(base_path.path().join("folder")).unwrap();
    File::create(base_path.path().join("folder").join("key2"))
      .unwrap()
      .write_all(b"value2")
      .unwrap();
    test(LocalStorage::new(base_path.path(), RegexResolver::new(".*", "$0").unwrap()).unwrap())
  }
}
