use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use futures_util::TryStreamExt;
use http::header::CONTENT_LENGTH;
use http::{HeaderMap, Method, Request, Uri};
use pin_project_lite::pin_project;
use reqwest::{Client, ClientBuilder};
use tokio_util::io::StreamReader;
use tracing::{debug, instrument};

use htsget_config::error;

use crate::StorageError::{InternalError, KeyNotFound, ResponseError, UrlParseError};
use crate::{
  GetOptions, HeadOptions, RangeUrlOptions, Result, StorageError, StorageMiddleware, StorageTrait,
};
use crate::{Streamable, Url as HtsGetUrl};

/// A storage struct which derives data from HTTP URLs.
#[derive(Debug, Clone)]
pub struct UrlStorage {
  client: Client,
  url: Uri,
  response_url: Uri,
  forward_headers: bool,
  header_blacklist: Vec<String>,
}

impl UrlStorage {
  /// Construct a new UrlStorage.
  pub fn new(
    client: Client,
    url: Uri,
    response_url: Uri,
    forward_headers: bool,
    header_blacklist: Vec<String>,
  ) -> Self {
    Self {
      client,
      url,
      response_url,
      forward_headers,
      header_blacklist,
    }
  }

  /// Construct a new UrlStorage with a default client.
  pub fn new_with_default_client(
    url: Uri,
    response_url: Uri,
    forward_headers: bool,
    header_blacklist: Vec<String>,
  ) -> Result<Self> {
    Ok(Self {
      client: ClientBuilder::new()
        .build()
        .map_err(|err| InternalError(format!("failed to build reqwest client: {}", err)))?,
      url,
      response_url,
      forward_headers,
      header_blacklist,
    })
  }

  /// Get a url from the key.
  pub fn get_url_from_key<K: AsRef<str> + Send>(&self, key: K) -> Result<Uri> {
    format!("{}{}", self.url, key.as_ref())
      .parse::<Uri>()
      .map_err(|err| UrlParseError(err.to_string()))
  }

  /// Get a url from the key.
  pub fn get_response_url_from_key<K: AsRef<str> + Send>(&self, key: K) -> Result<Uri> {
    format!("{}{}", self.response_url, key.as_ref())
      .parse::<Uri>()
      .map_err(|err| UrlParseError(err.to_string()))
  }

  /// Remove blacklisted headers from the headers.
  pub fn remove_blacklisted_headers(&self, mut headers: HeaderMap) -> HeaderMap {
    for blacklisted_header in &self.header_blacklist {
      headers.remove(blacklisted_header);
    }
    headers
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

    let request = Request::builder().method(method).uri(&url);

    let request = headers
      .iter()
      .fold(request, |acc, (key, value)| acc.header(key, value))
      .body(vec![])
      .map_err(|err| UrlParseError(err.to_string()))?;

    let response = self
      .client
      .execute(
        request
          .try_into()
          .map_err(|err| InternalError(format!("failed to create http request: {}", err)))?,
      )
      .await
      .map_err(|err| KeyNotFound(format!("{} with key {}", err, key)))?;

    let status = response.status();
    if status.is_client_error() || status.is_server_error() {
      Err(KeyNotFound(format!(
        "url returned {} for key {}",
        status, key
      )))
    } else {
      Ok(response)
    }
  }

  /// Construct and send a request
  pub fn format_url<K: AsRef<str> + Send>(
    &self,
    key: K,
    options: RangeUrlOptions<'_>,
  ) -> Result<HtsGetUrl> {
    let url = self.get_response_url_from_key(key)?.into_parts();
    let url = Uri::from_parts(url)
      .map_err(|err| InternalError(format!("failed to convert to uri from parts: {}", err)))?;

    let mut url = HtsGetUrl::new(url.to_string());
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

pin_project! {
  /// A wrapper around a stream used by `UrlStorage`.
  pub struct UrlStream {
    #[pin]
    inner: Box<dyn Stream<Item = Result<Bytes>> + Unpin + Send + Sync>
  }
}

impl UrlStream {
  /// Create a new UrlStream.
  pub fn new(inner: Box<dyn Stream<Item = Result<Bytes>> + Unpin + Send + Sync>) -> Self {
    Self { inner }
  }
}

impl Stream for UrlStream {
  type Item = Result<Bytes>;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.project().inner.poll_next(cx)
  }
}

#[async_trait]
impl StorageMiddleware for UrlStorage {}

#[async_trait]
impl StorageTrait for UrlStorage {
  #[instrument(level = "trace", skip(self))]
  async fn get(&self, key: &str, options: GetOptions<'_>) -> Result<Streamable> {
    debug!(calling_from = ?self, key, "getting file with key {:?}", key);

    let request_headers = self.remove_blacklisted_headers(options.request_headers().clone());
    let response = self.get_key(key.to_string(), &request_headers).await?;

    Ok(Streamable::from_async_read(StreamReader::new(
      UrlStream::new(Box::new(response.bytes_stream().map_err(|err| {
        ResponseError(format!("reading body from response: {}", err))
      }))),
    )))
  }

  #[instrument(level = "trace", skip(self))]
  async fn range_url(&self, key: &str, options: RangeUrlOptions<'_>) -> Result<HtsGetUrl> {
    debug!(calling_from = ?self, key, "getting url with key {:?}", key);

    let response_headers = self.remove_blacklisted_headers(options.response_headers().clone());
    let new_options = RangeUrlOptions::new(options.range().clone(), &response_headers);

    self.format_url(key, new_options)
  }

  #[instrument(level = "trace", skip(self))]
  async fn head(&self, key: &str, options: HeadOptions<'_>) -> Result<u64> {
    let request_headers = self.remove_blacklisted_headers(options.request_headers().clone());
    let head = self.head_key(key, &request_headers).await?;

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
  use std::path::Path;
  use std::str::FromStr;
  use std::{result, vec};

  use axum::body::Body;
  use axum::middleware::Next;
  use axum::response::Response;
  use axum::{middleware, Router};
  use http::header::{AUTHORIZATION, HOST};
  use http::{HeaderName, HeaderValue, Request, StatusCode};
  use tokio::io::AsyncReadExt;
  use tokio::net::TcpListener;
  use tower_http::services::ServeDir;

  use htsget_config::types::Headers;

  use crate::local::tests::create_local_test_files;

  use super::*;

  #[test]
  fn get_url_from_key() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8080").unwrap(),
      true,
      vec![],
    );

    assert_eq!(
      storage.get_url_from_key("assets/key1").unwrap(),
      Uri::from_str("https://example.com/assets/key1").unwrap()
    );
  }

  #[test]
  fn get_response_url_from_key() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8080").unwrap(),
      true,
      vec![],
    );

    assert_eq!(
      storage.get_response_url_from_key("assets/key1").unwrap(),
      Uri::from_str("https://localhost:8080/assets/key1").unwrap()
    );
  }

  #[test]
  fn remove_blacklisted_headers() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8080").unwrap(),
      true,
      vec![HOST.to_string()],
    );

    let mut headers = HeaderMap::default();
    headers.insert(
      HeaderName::from_str(HOST.as_str()).unwrap(),
      HeaderValue::from_str("example.com").unwrap(),
    );
    headers.insert(
      HeaderName::from_str(AUTHORIZATION.as_str()).unwrap(),
      HeaderValue::from_str("secret").unwrap(),
    );

    let headers = storage.remove_blacklisted_headers(headers.clone());

    assert_eq!(headers.len(), 1);
  }

  #[tokio::test]
  async fn send_request() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        vec![],
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
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        vec![],
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
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        vec![],
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
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        vec![],
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
      let storage = UrlStorage::new(
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        vec![],
      );

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
  async fn range_url_storage_blacklisted_headers() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        vec![HOST.to_string()],
      );

      let mut headers = HeaderMap::default();
      headers.insert(
        HeaderName::from_str(HOST.as_str()).unwrap(),
        HeaderValue::from_str("example.com").unwrap(),
      );

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
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        vec![],
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
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8080").unwrap(),
      true,
      vec![],
    );

    let mut headers = HeaderMap::default();
    let options = test_range_options(&mut headers);

    assert_eq!(
      storage.format_url("assets/key1", options).unwrap(),
      HtsGetUrl::new("https://localhost:8080/assets/key1")
        .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
    );
  }

  #[test]
  fn format_url_different_response_scheme() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("http://example.com").unwrap(),
      true,
      vec![],
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
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8081").unwrap(),
      false,
      vec![],
    );

    let mut headers = HeaderMap::default();
    let options = test_range_options(&mut headers);

    assert_eq!(
      storage.format_url("assets/key1", options).unwrap(),
      HtsGetUrl::new("https://localhost:8081/assets/key1")
    );
  }

  fn test_client() -> Client {
    ClientBuilder::new().build().unwrap()
  }

  pub(crate) async fn with_url_test_server<F, Fut>(test: F)
  where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    with_test_server(base_path.path(), test).await;
  }

  async fn test_auth(request: Request<Body>, next: Next) -> result::Result<Response, StatusCode> {
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
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, router.into_make_service()).await });

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
