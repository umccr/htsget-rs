//! Provides the storage abstraction for [HtsGet].
//!

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncSeek};

use crate::htsget::Url;
use crate::storage::{GetOptions, UrlOptions};

use super::Result;

/// A Storage represents some kind of object based storage (either locally or in the cloud)
/// that can be used to retrieve files for alignments, variants or its respective indexes.
#[async_trait]
pub trait AsyncStorage {
  type Streamable: AsyncRead + AsyncSeek + Unpin + Send;

  async fn get<K: AsRef<str> + Send>(
    &self,
    key: K,
    options: GetOptions,
  ) -> Result<Self::Streamable>;

  async fn url<K: AsRef<str> + Send>(&self, key: K, options: UrlOptions) -> Result<Url>;

  async fn head<K: AsRef<str> + Send>(&self, key: K) -> Result<u64>;
}
