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

use htsget_config::types::{Headers, Scheme};

use crate::storage::StorageError::{InternalError, KeyNotFound, ResponseError, UrlParseError};
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

  /// Construct and send a request
  pub fn format_url<K: AsRef<str> + Send>(
    &self,
    key: K,
    options: RangeUrlOptions<'_>,
  ) -> Result<HtsGetUrl> {
    let mut url = self.get_url_from_key(key)?;

    url
      .set_scheme(&self.response_scheme.to_string())
      .map_err(|_| {
        InternalError("failed to set scheme when formatting response url".to_string())
      })?;

    let mut url = HtsGetUrl::new(url);
    if self.forward_headers {
      url = url.with_headers(options.response_headers().iter().try_fold(
        Headers::default(),
        |acc, (key, value)| {
          Ok::<_, StorageError>(acc.with_header(
            key.to_string(),
            value.to_str().map_err(|err| {
              InternalError(format!("failed to convert header value to string: {}", err))
            })?,
          ))
        },
      )?)
    }

    Ok(options.apply(url))
  }

  /// Get the head from the key.
  pub async fn head_key<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
  ) -> Result<reqwest::Response> {
    self.send_request(key, headers, Method::HEAD).await
  }

  /// Get the key.
  pub async fn get_key<K: AsRef<str> + Send>(
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
      .get_key(key.to_string(), options.request_headers())
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
    key: K,
    options: RangeUrlOptions<'_>,
  ) -> Result<HtsGetUrl> {
    let key = key.as_ref();
    debug!(calling_from = ?self, key, "getting url with key {:?}", key);

    self.format_url(key, options)
  }

  #[instrument(level = "trace", skip(self))]
  async fn head<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: HeadOptions<'_>,
  ) -> Result<u64> {
    let key = key.as_ref();
    let head = self.head_key(key, options.request_headers()).await?;

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

#[cfg(test)]
mod tests {
  use std::future::Future;
  use std::str::FromStr;

  use http::{HeaderName, HeaderValue};
  use mockito::Server;

  use super::*;

  #[test]
  fn get_url_from_key() {
    let storage = UrlStorage::new(
      Client::new(),
      Url::parse("https://example.com").unwrap(),
      Scheme::Https,
      true,
    );

    assert_eq!(
      storage.get_url_from_key("test.bam").unwrap(),
      Url::parse("https://example.com/test.bam").unwrap()
    );
  }

  #[tokio::test]
  async fn send_request() {
    with_test_server(|url| async move {
      let storage = UrlStorage::new(
        Client::new(),
        Url::parse(&url).unwrap(),
        Scheme::Https,
        true,
      );

      let headers = HeaderMap::default();

      let response = String::from_utf8(
        storage
          .send_request("test.bam", &headers, Method::GET)
          .await
          .unwrap()
          .bytes()
          .await
          .unwrap()
          .to_vec(),
      )
      .unwrap();
      assert_eq!(response, "body");
    })
    .await;
  }

  #[tokio::test]
  async fn get_key() {
    with_test_server(|url| async move {
      let storage = UrlStorage::new(
        Client::new(),
        Url::parse(&url).unwrap(),
        Scheme::Https,
        true,
      );

      let headers = HeaderMap::default();

      let response = String::from_utf8(
        storage
          .get_key("test.bam", &headers)
          .await
          .unwrap()
          .bytes()
          .await
          .unwrap()
          .to_vec(),
      )
      .unwrap();
      assert_eq!(response, "body");
    })
    .await;
  }

  #[tokio::test]
  async fn head_key() {
    with_test_server(|url| async move {
      let storage = UrlStorage::new(
        Client::new(),
        Url::parse(&url).unwrap(),
        Scheme::Https,
        true,
      );

      let headers = HeaderMap::default();

      let response = storage
        .get_key("test.bam", &headers)
        .await
        .unwrap()
        .content_length()
        .unwrap();
      assert_eq!(response, 4);
    })
    .await;
  }

  #[test]
  fn format_url() {
    let storage = UrlStorage::new(
      Client::new(),
      Url::parse("https://example.com").unwrap(),
      Scheme::Https,
      true,
    );

    let mut headers = HeaderMap::default();
    let options = test_range_options(&mut headers);

    assert_eq!(
      storage.format_url("test.bam", options).unwrap(),
      HtsGetUrl::new("https://example.com/test.bam")
        .with_headers(Headers::default().with_header("authorization", "secret"))
    );
  }

  #[test]
  fn format_url_different_response_scheme() {
    let storage = UrlStorage::new(
      Client::new(),
      Url::parse("https://example.com").unwrap(),
      Scheme::Http,
      true,
    );

    let mut headers = HeaderMap::default();
    let options = test_range_options(&mut headers);

    assert_eq!(
      storage.format_url("test.bam", options).unwrap(),
      HtsGetUrl::new("http://example.com/test.bam")
        .with_headers(Headers::default().with_header("authorization", "secret"))
    );
  }

  #[test]
  fn format_url_no_headers() {
    let storage = UrlStorage::new(
      Client::new(),
      Url::parse("https://example.com").unwrap(),
      Scheme::Https,
      false,
    );

    let mut headers = HeaderMap::default();
    let options = test_range_options(&mut headers);

    assert_eq!(
      storage.format_url("test.bam", options).unwrap(),
      HtsGetUrl::new("https://example.com/test.bam")
    );
  }

  pub(crate) async fn with_test_server<F, Fut>(test: F)
  where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = ()>,
  {
    let mut server = Server::new();

    let mock = server
      .mock("GET", "/test.bam")
      .with_status(201)
      .with_header("content-type", "text/plain")
      .with_header("Authorization", "secret")
      .with_body("body")
      .create();

    mock.expect(1);

    test(server.url()).await;
  }

  fn test_range_options(headers: &mut HeaderMap) -> RangeUrlOptions {
    headers.append(
      HeaderName::from_str("authorization").unwrap(),
      HeaderValue::from_str("secret").unwrap(),
    );
    let options = RangeUrlOptions::new_with_default_range(headers);

    options
  }
}
