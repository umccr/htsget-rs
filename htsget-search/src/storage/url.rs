use std::fmt::Debug;

use async_trait::async_trait;
use http::HeaderMap;
use reqwest::{Client, Error, RequestBuilder, Url};
use tokio::fs::File;
use tracing::instrument;

use htsget_config::types::Scheme;

use crate::storage::StorageError::{KeyNotFound, ResponseError, UrlParseError};
use crate::storage::{GetOptions, HeadOptions, RangeUrlOptions, Result, Storage, StorageError};
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

  fn map_err(error: Error, key: &str) -> StorageError {
    match error.status() {
      None => KeyNotFound(key.to_string()),
      Some(status) => KeyNotFound(format!("for {} with status: {}", key, status)),
    }
  }

  fn apply_headers(builder: RequestBuilder, headers: &HeaderMap) -> RequestBuilder {
    headers
      .iter()
      .fold(builder, |builder, (key, value)| builder.header(key, value))
  }

  /// Get a url from the key.
  pub fn get_url_from_key<K: AsRef<str> + Send + Debug>(&self, key: K) -> Result<Url> {
    self
      .url
      .join(key.as_ref())
      .map_err(|err| UrlParseError(err.to_string()))
  }

  /// Get the head from the key.
  pub async fn head_url<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    headers: &HeaderMap,
  ) -> Result<reqwest::Response> {
    let key = key.as_ref();

    let url = self.get_url_from_key(key)?;

    let builder = self.client.head(url);
    let builder = Self::apply_headers(builder, headers);

    builder.send().await.map_err(|err| Self::map_err(err, key))
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
    key: K,
    options: HeadOptions<'_>,
  ) -> Result<u64> {
    let key = key.as_ref();
    let head = self.head_url(key, options.request_headers).await?;

    head.content_length().ok_or_else(|| {
      ResponseError(format!(
        "no content length in head response for key: {}",
        key
      ))
    })
  }
}
