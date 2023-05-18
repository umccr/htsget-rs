use std::fmt::Debug;

use async_trait::async_trait;
use reqwest::{Client, Url};
use tokio::fs::File;
use tracing::instrument;

use htsget_config::types::Scheme;

use crate::storage::StorageError::UrlParseError;
use crate::storage::{GetOptions, HeadOptions, RangeUrlOptions, Result, Storage};
use crate::Url as HtsGetUrl;

/// A storage struct which derives data from HTTP URLs.
#[derive(Debug, Clone)]
pub struct UrlStorage {
  client: Client,
  url: Url,
  response_scheme: Scheme,
  forward_headers: bool,
}

impl UrlStorage {
  /// Construct a new UrlStorage.
  pub fn new(client: Client, url: Url, response_scheme: Scheme, forward_headers: bool) -> Self {
    Self {
      client,
      url,
      response_scheme,
      forward_headers,
    }
  }

  /// Construct a new UrlStorage with a string url.
  pub fn try_from_str(
    client: Client,
    url: &str,
    response_scheme: Scheme,
    forward_headers: bool,
  ) -> Result<Self> {
    Ok(Self::new(
      client,
      Url::parse(url).map_err(|err| UrlParseError(err.to_string()))?,
      response_scheme,
      forward_headers,
    ))
  }
}

#[async_trait]
impl Storage for UrlStorage {
  type Streamable = File;

  #[instrument(level = "trace", skip(self))]
  async fn get<K: AsRef<str> + Send + Debug>(
    &self,
    _key: K,
    _options: GetOptions<'_>,
  ) -> Result<Self::Streamable> {
    todo!()
  }

  #[instrument(level = "trace", skip(self))]
  async fn range_url<K: AsRef<str> + Send + Debug>(
    &self,
    _key: K,
    _options: RangeUrlOptions<'_>,
  ) -> Result<HtsGetUrl> {
    todo!()
  }

  #[instrument(level = "trace", skip(self))]
  async fn head<K: AsRef<str> + Send + Debug>(
    &self,
    _key: K,
    _options: HeadOptions<'_>,
  ) -> Result<u64> {
    todo!()
  }
}
