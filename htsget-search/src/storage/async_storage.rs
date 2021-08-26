use std::path::PathBuf;

use async_trait::async_trait;

use crate::htsget::Url;
use crate::storage::{GetOptions, UrlOptions};

use super::Result;

/// A Storage represents some kind of object based storage (either locally or in the cloud)
/// that can be used to retrieve files for alignments, variants or its respective indexes.
#[async_trait]
pub trait AsyncStorage {
  // TODO Consider another type of interface based on IO streaming
  // so we don't need to guess the length of the headers, but just
  // parse them in an streaming fashion.
  async fn get<K: AsRef<str> + Send>(&self, key: K, options: GetOptions) -> Result<PathBuf>;

  async fn url<K: AsRef<str> + Send>(&self, key: K, options: UrlOptions) -> Result<Url>;

  async fn head<K: AsRef<str> + Send>(&self, key: K) -> Result<u64>;
}
