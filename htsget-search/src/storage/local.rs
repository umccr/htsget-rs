use std::path::{Path, PathBuf};

use super::{Result, Storage, StorageError};

pub struct LocalStorage {
  base_path: PathBuf,
}

impl LocalStorage {
  pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
    let base_path: &Path = base_path.as_ref();
    Self {
      base_path: base_path.to_path_buf(),
    }
  }
}

impl Storage for LocalStorage {
  fn get<K: AsRef<str>>(&self, key: K, _length: Option<usize>) -> Result<PathBuf> {
    let key: &str = key.as_ref();
    self
      .base_path
      .join(key)
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
          .exists()
          .then(|| path)
          .ok_or_else(|| StorageError::NotFound(key.to_string()))
      })
  }
}
