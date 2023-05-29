use std::fmt::{Debug, Display};
use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use futures_util::TryStreamExt;
use http::header::CONTENT_LENGTH;
use http::{HeaderMap, Method};
use reqwest::{Client, Error, RequestBuilder, Url};
use tokio_util::io::StreamReader;
use tracing::{debug, instrument};

use htsget_config::error;
use htsget_config::types::Scheme;

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
      url = url.with_headers(
        options
          .response_headers()
          .try_into()
          .map_err(|err: error::Error| StorageError::InvalidInput(err.to_string()))?,
      )
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

    // Interestingly, reqwest's `content_length` function does not return the content-length header.
    // See https://github.com/seanmonstar/reqwest/issues/843.
    let len = head
      .headers()
      .get(CONTENT_LENGTH)
      .and_then(|content_length| content_length.to_str().ok())
      .and_then(|content_length| content_length.parse().ok())
      .ok_or_else(|| {
        ResponseError(format!(
          "failed to get content length from head response for key: {}",
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
  use std::net::TcpListener;
  use std::path::Path;
  use std::result;
  use std::str::FromStr;

  use axum::middleware::Next;
  use axum::response::Response;
  use axum::{middleware, Router};
  use http::header::AUTHORIZATION;
  use http::{HeaderName, HeaderValue, Request, StatusCode};
  use tokio::io::AsyncReadExt;
  use tower_http::services::ServeDir;

  use htsget_config::types::Headers;

  use crate::storage::local::tests::create_local_test_files;

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
      storage.get_url_from_key("assets/key1").unwrap(),
      Url::parse("https://example.com/assets/key1").unwrap()
    );
  }

  #[tokio::test]
  async fn send_request() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        Client::new(),
        Url::parse(&url).unwrap(),
        Scheme::Https,
        true,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response = String::from_utf8(
        storage
          .send_request("assets/key1", headers, Method::GET)
          .await
          .unwrap()
          .bytes()
          .await
          .unwrap()
          .to_vec(),
      )
      .unwrap();
      assert_eq!(response, "value1");
    })
    .await;
  }

  #[tokio::test]
  async fn get_key() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        Client::new(),
        Url::parse(&url).unwrap(),
        Scheme::Https,
        true,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response = String::from_utf8(
        storage
          .get_key("assets/key1", headers)
          .await
          .unwrap()
          .bytes()
          .await
          .unwrap()
          .to_vec(),
      )
      .unwrap();
      assert_eq!(response, "value1");
    })
    .await;
  }

  #[tokio::test]
  async fn head_key() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        Client::new(),
        Url::parse(&url).unwrap(),
        Scheme::Https,
        true,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response: u64 = storage
        .get_key("assets/key1", headers)
        .await
        .unwrap()
        .headers()
        .get(CONTENT_LENGTH)
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
      assert_eq!(response, 6);
    })
    .await;
  }

  #[tokio::test]
  async fn get_storage() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        Client::new(),
        Url::parse(&url).unwrap(),
        Scheme::Https,
        true,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);
      let options = GetOptions::new_with_default_range(headers);

      let mut reader = storage.get("assets/key1", options).await.unwrap();

      let mut response = [0; 6];
      reader.read_exact(&mut response).await.unwrap();

      assert_eq!(String::from_utf8(response.to_vec()).unwrap(), "value1");
    })
    .await;
  }

  #[tokio::test]
  async fn range_url_storage() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(Client::new(), Url::parse(&url).unwrap(), Scheme::Http, true);

      let mut headers = HeaderMap::default();
      let options = test_range_options(&mut headers);

      assert_eq!(
        storage.range_url("assets/key1", options).await.unwrap(),
        HtsGetUrl::new(format!("{}/assets/key1", url))
          .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn head_storage() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        Client::new(),
        Url::parse(&url).unwrap(),
        Scheme::Https,
        true,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);
      let options = HeadOptions::new(headers);

      assert_eq!(storage.head("assets/key1", options).await.unwrap(), 6);
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
      storage.format_url("assets/key1", options).unwrap(),
      HtsGetUrl::new("https://example.com/assets/key1")
        .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
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
      storage.format_url("assets/key1", options).unwrap(),
      HtsGetUrl::new("http://example.com/assets/key1")
        .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
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
      storage.format_url("assets/key1", options).unwrap(),
      HtsGetUrl::new("https://example.com/assets/key1")
    );
  }

  pub(crate) async fn with_url_test_server<F, Fut>(test: F)
  where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    with_test_server(base_path.path(), test).await;
  }

  async fn test_auth<B>(
    request: Request<B>,
    next: Next<B>,
  ) -> result::Result<Response, StatusCode> {
    let auth_header = request
      .headers()
      .get(AUTHORIZATION)
      .and_then(|header| header.to_str().ok());

    match auth_header {
      Some("secret") => Ok(next.run(request).await),
      _ => Err(StatusCode::UNAUTHORIZED),
    }
  }

  pub(crate) async fn with_test_server<F, Fut>(server_base_path: &Path, test: F)
  where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = ()>,
  {
    let router = Router::new()
      .nest_service("/assets", ServeDir::new(server_base_path.to_str().unwrap()))
      .route_layer(middleware::from_fn(test_auth));

    // TODO fix this in htsget-test to bind and return tcp listener.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(
      axum::Server::from_tcp(listener)
        .unwrap()
        .serve(router.into_make_service()),
    );

    test(format!("http://{}", addr)).await;
  }

  fn test_headers(headers: &mut HeaderMap) -> &HeaderMap {
    headers.append(
      HeaderName::from_str(AUTHORIZATION.as_str()).unwrap(),
      HeaderValue::from_str("secret").unwrap(),
    );
    headers
  }

  fn test_range_options(headers: &mut HeaderMap) -> RangeUrlOptions {
    let headers = test_headers(headers);
    let options = RangeUrlOptions::new_with_default_range(headers);

    options
  }
}
