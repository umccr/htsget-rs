use std::fmt::{Debug, Display};
use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use futures_util::TryStreamExt;
use http::{HeaderMap, Method};
use reqwest::{Client, Error, RequestBuilder, Url};
use tokio_util::io::StreamReader;
use tracing::{debug, instrument};

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

  fn map_err<K: Display>(key: K, error: Error) -> StorageError {
    match error.status() {
      None => KeyNotFound(format!("{}", key)),
      Some(status) => KeyNotFound(format!("for {} with status: {}", key, status)),
    }
  }

  fn apply_headers(builder: RequestBuilder, headers: &HeaderMap) -> RequestBuilder {
    headers
      .iter()
      .fold(builder, |builder, (key, value)| builder.header(key, value))
  }

  /// Get a url from the key.
  pub fn get_url_from_key<K: AsRef<str> + Send>(&self, key: K) -> Result<Url> {
    self
      .url
      .join(key.as_ref())
      .map_err(|err| UrlParseError(err.to_string()))
  }

  /// Construct and send a request
  pub async fn send_request<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
    method: Method,
  ) -> Result<reqwest::Response> {
    let key = key.as_ref();
    let url = self.get_url_from_key(key)?;
    let url_key = url.to_string();

    let builder = self.client.request(method, url);
    let builder = Self::apply_headers(builder, headers);

    builder
      .send()
      .await
      .map_err(|err| Self::map_err(url_key, err))
  }

  /// Get the head from the key.
  pub async fn head_url<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
  ) -> Result<reqwest::Response> {
    self.send_request(key, headers, Method::HEAD).await
  }

  /// Get the key.
  pub async fn get_url<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
  ) -> Result<reqwest::Response> {
    self.send_request(key, headers, Method::GET).await
  }
}

#[async_trait]
impl Storage for UrlStorage {
  // There might be a nicer way to express this type.
  type Streamable = StreamReader<Pin<Box<dyn Stream<Item = Result<Bytes>> + Send + Sync>>, Bytes>;

  #[instrument(level = "trace", skip(self))]
  async fn get<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: GetOptions<'_>,
  ) -> Result<Self::Streamable> {
    let key = key.as_ref().to_string();
    debug!(calling_from = ?self, key, "getting file with key {:?}", key);

    let response = self
      .get_url(key.to_string(), options.request_headers)
      .await?;
    let url = response.url().to_string();

    Ok(StreamReader::new(Box::pin(
      response
        .bytes_stream()
        .map_err(move |err| Self::map_err(url.to_string(), err)),
    )))
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

    let len = head.content_length().ok_or_else(|| {
      ResponseError(format!(
        "no content length in head response for key: {}",
        key
      ))
    })?;

    debug!(calling_from = ?self, key, len, "size of key {:?} is {}", key, len);
    Ok(len)
  }
}
