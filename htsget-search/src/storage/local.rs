//! Module providing an implementation for the [Storage] trait using the local file system.
//!

use std::path::{Path, PathBuf};

use super::{GetOptions, Result, Storage, StorageError};

/// Implementation for the [Storage] trait using the local file system.
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
  fn get<K: AsRef<str>>(&self, key: K, _options: GetOptions) -> Result<PathBuf> {
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

#[cfg(test)]
mod tests {

  use super::*;

  #[test]
  fn get_() {
    // TODO determine root path through cargo env vars
    let storage = LocalStorage::new("../data");
  }
}
