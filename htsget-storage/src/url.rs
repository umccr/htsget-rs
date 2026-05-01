use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use futures_util::TryStreamExt;
use http::header::{CONTENT_LENGTH, RANGE};
use http::{HeaderMap, Method, Request, Uri};
use pin_project_lite::pin_project;
use reqwest_middleware::ClientWithMiddleware;
use tokio_util::io::StreamReader;
use tracing::{debug, instrument};

use htsget_config::error;

use crate::StorageError::{InternalError, KeyNotFound, ResponseError, UrlParseError};
use crate::types::{BytesPosition, BytesRange};
use crate::{
  GetOptions, HeadOptions, RangeUrlOptions, Result, StorageError, StorageMiddleware, StorageTrait,
};
use crate::{Streamable, Url as HtsGetUrl};
use wildmatch::WildMatch;

/// A wrapper around a HTTP client to perform requests when htsget-rs acts as a client.
#[derive(Debug, Clone)]
pub struct UrlClient {
  client: ClientWithMiddleware,
  forward_headers_backend: Vec<WildMatch>,
  reflect_headers_client: Vec<WildMatch>,
}

impl UrlClient {
  /// Construct a new `UrlClient`.
  pub fn new(
    client: ClientWithMiddleware,
    forward_headers_backend: Vec<String>,
    reflect_headers_client: Vec<String>,
  ) -> Self {
    let map_filters = |filters: Vec<String>| filters.iter().map(|f| WildMatch::new(f)).collect();
    Self {
      client,
      forward_headers_backend: map_filters(forward_headers_backend),
      reflect_headers_client: map_filters(reflect_headers_client),
    }
  }

  /// Filter a header map, keeping only headers whose names match at least one of the given patterns.
  fn filter_headers(headers: HeaderMap, patterns: &[WildMatch]) -> HeaderMap {
    if patterns.is_empty() {
      return HeaderMap::new();
    }

    headers
        .into_iter()
        .filter_map(|(name, value)| {
          let name = name?;
          if patterns.iter().any(|pattern| pattern.matches(name.as_str())) {
            Some((name, value))
          } else {
            None
          }
        })
        .collect()
  }

  /// Filter headers to only those matching the `forward_headers_backend` patterns.
  pub fn filter_forward_headers(&self, headers: HeaderMap) -> HeaderMap {
    Self::filter_headers(headers, &self.forward_headers_backend)
  }

  /// Filter headers to only those matching the `reflect_headers_client` patterns.
  pub fn filter_reflect_headers(&self, headers: HeaderMap) -> HeaderMap {
    Self::filter_headers(headers, &self.reflect_headers_client)
  }

  /// Construct and send a request according to htsget positions and headers.
  pub async fn send_request(
    &self,
    request_url: Uri,
    position: BytesPosition,
    headers: HeaderMap,
    method: Method,
  ) -> Result<reqwest::Response> {
    let request_headers = self.filter_forward_headers(headers);
    let request = Request::builder().method(method).uri(&request_url);

    let range = BytesRange::from(&position).to_string();
    let request = request_headers
      .iter()
      .fold(request, |acc, (key, value)| acc.header(key, value));

    let request = if !range.is_empty() {
      request.header(RANGE, &range)
    } else {
      request
    };

    let request = request
      .body(vec![])
      .map_err(|err| UrlParseError(err.to_string()))?;

    let response = self
      .client
      .execute(
        request
          .try_into()
          .map_err(|err| InternalError(format!("failed to create http request: {err}")))?,
      )
      .await
      .map_err(|err| KeyNotFound(format!("{err} with url {request_url}")))?;

    let status = response.status();
    if status.is_client_error() || status.is_server_error() {
      Err(KeyNotFound(format!(
        "url returned {status} for url {request_url}"
      )))
    } else {
      Ok(response)
    }
  }

  /// Construct htsget tickets to return to the client.
  pub fn format_url(&self, returned_url: Uri, options: RangeUrlOptions<'_>) -> Result<HtsGetUrl> {
    let response_headers = self.filter_reflect_headers(options.response_headers().clone());

    let url = Uri::from_parts(returned_url.into_parts())
      .map_err(|err| InternalError(format!("failed to convert to uri from parts: {err}")))?;

    let mut url = HtsGetUrl::new(url.to_string());
    if !response_headers.is_empty() {
      url = url.with_headers(
        (&response_headers)
          .try_into()
          .map_err(|err: error::Error| StorageError::InvalidInput(err.to_string()))?,
      )
    }

    Ok(options.apply(url))
  }

  /// Extract the object size from a response.
  pub fn extract_size(response: reqwest::Response) -> Result<u64> {
    response
      .headers()
      .get(CONTENT_LENGTH)
      .and_then(|content_length| content_length.to_str().ok())
      .and_then(|content_length| content_length.parse().ok())
      .ok_or_else(|| ResponseError("failed to get content length from head response".to_string()))
  }

  /// Append the requested key to the url.
  pub fn append_key_to_url<K: AsRef<str>>(&self, base: &Uri, key: K) -> Result<Uri> {
    format!("{}{}", base, key.as_ref())
      .parse::<Uri>()
      .map_err(|err| UrlParseError(err.to_string()))
  }
}

/// A storage struct which derives data from HTTP URLs.
#[derive(Debug, Clone)]
pub struct UrlStorage {
  url: Uri,
  response_url: Uri,
  url_client: UrlClient,
}

impl UrlStorage {
  /// Construct a new UrlStorage.
  pub fn new(
    client: ClientWithMiddleware,
    url: Uri,
    response_url: Uri,
    forward_headers_backend: Vec<String>,
    reflect_headers_client: Vec<String>,
  ) -> Self {
    Self {
      url,
      response_url,
      url_client: UrlClient::new(client, forward_headers_backend, reflect_headers_client),
    }
  }

  /// Get a url from the key.
  pub fn get_url_from_key<K: AsRef<str> + Send>(&self, key: K) -> Result<Uri> {
    self.url_client.append_key_to_url(&self.url, key)
  }

  /// Get the response url from the key.
  pub fn get_response_url_from_key<K: AsRef<str> + Send>(&self, key: K) -> Result<Uri> {
    self.url_client.append_key_to_url(&self.response_url, key)
  }

  /// Get the head from the key.
  pub async fn head_key<K: AsRef<str> + Send>(
    &self,
    key: K,
    options: HeadOptions<'_>,
  ) -> Result<reqwest::Response> {
    let url = self.get_url_from_key(key)?;
    self
      .url_client
      .send_request(
        url,
        Default::default(),
        options.request_headers().clone(),
        Method::HEAD,
      )
      .await
  }

  /// Get the key.
  pub async fn get_key<K: AsRef<str> + Send>(
    &self,
    key: K,
    options: GetOptions<'_>,
  ) -> Result<reqwest::Response> {
    let url = self.get_url_from_key(key)?;
    let headers = options.request_headers().clone();
    self
      .url_client
      .send_request(url, options.range, headers, Method::GET)
      .await
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

  /// Create a streamable type from a response.
  pub fn streamable_from_response(response: reqwest::Response) -> Streamable {
    Streamable::from_async_read(StreamReader::new(UrlStream::new(Box::new(
      response
        .bytes_stream()
        .map_err(|err| ResponseError(format!("reading body from response: {err}"))),
    ))))
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

    let response = self.get_key(key.to_string(), options).await?;
    Ok(UrlStream::streamable_from_response(response))
  }

  #[instrument(level = "trace", skip(self))]
  async fn range_url(&self, key: &str, options: RangeUrlOptions<'_>) -> Result<HtsGetUrl> {
    debug!(calling_from = ?self, key, "getting url with key {:?}", key);

    self
      .url_client
      .format_url(self.get_response_url_from_key(key)?, options)
  }

  #[instrument(level = "trace", skip(self))]
  async fn head(&self, key: &str, options: HeadOptions<'_>) -> Result<u64> {
    let head = self.head_key(key, options).await?;

    let len = UrlClient::extract_size(head)?;

    debug!(calling_from = ?self, key, len, "size of key {:?} is {}", key, len);
    Ok(len)
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use reqwest::ClientBuilder;
  use std::future::Future;
  use std::path::{Path, PathBuf};
  use std::str::FromStr;
  use std::{result, vec};

  use axum::body::Body;
  use axum::middleware::Next;
  use axum::response::Response;
  use axum::{Router, middleware};
  use http::header::{AUTHORIZATION, HOST};
  use http::{HeaderName, HeaderValue, Request, StatusCode};
  use reqwest_middleware::ClientWithMiddleware;
  use tokio::io::AsyncReadExt;
  use tokio::net::TcpListener;
  use tower_http::services::ServeDir;

  use htsget_config::types::Headers;

  use super::*;
  use crate::local::tests::create_local_test_files;
  use crate::types::GetOptions;

  #[test]
  fn get_url_from_key() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8080").unwrap(),
      vec!["*".to_string()],
      vec!["*".to_string()],
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
      vec!["*".to_string()],
      vec!["*".to_string()],
    );

    assert_eq!(
      storage.get_response_url_from_key("assets/key1").unwrap(),
      Uri::from_str("https://localhost:8080/assets/key1").unwrap()
    );
  }

  #[test]
  fn filter_forward_headers_wildcard() {
    let storage = UrlClient::new(
      test_client(),
      vec!["authorization".to_string()],
      vec!["*".to_string()],
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

    let headers = storage.filter_forward_headers(headers);

    assert_eq!(headers.len(), 1);
    assert!(headers.get(AUTHORIZATION).is_some());
  }

  #[tokio::test]
  async fn send_request() {
    with_url_test_server(|storage, _, _| async move {
      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response = String::from_utf8(
        storage
          .url_client
          .send_request(
            storage.get_url_from_key("assets/key1").unwrap(),
            Default::default(),
            headers.clone(),
            Method::GET,
          )
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
    with_url_test_server(|storage, _, _| async move {
      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response = String::from_utf8(
        storage
          .get_key("assets/key1", GetOptions::new_with_default_range(headers))
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
    with_url_test_server(|storage, _, _| async move {
      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response: u64 = storage
        .get_key("assets/key1", GetOptions::new_with_default_range(headers))
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
    with_url_test_server(|storage, _, _| async move {
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
    with_url_test_server(|_, url, _| async move {
      let storage = UrlStorage::new(
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        vec!["*".to_string()],
        vec!["*".to_string()],
      );
      let mut headers = HeaderMap::default();
      let options = test_range_options(&mut headers);

      assert_eq!(
        storage.range_url("assets/key1", options).await.unwrap(),
        HtsGetUrl::new(format!("{url}/assets/key1"))
          .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn range_url_storage_filtered_headers() {
    with_url_test_server(|_, url, _| async move {
      let storage = UrlStorage::new(
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        vec!["*".to_string()],
        vec!["authorization".to_string()],
      );

      let mut headers = HeaderMap::default();
      headers.insert(
        HeaderName::from_str(HOST.as_str()).unwrap(),
        HeaderValue::from_str("example.com").unwrap(),
      );

      let options = test_range_options(&mut headers);

      assert_eq!(
        storage.range_url("assets/key1", options).await.unwrap(),
        HtsGetUrl::new(format!("{url}/assets/key1"))
          .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn head_storage() {
    with_url_test_server(|storage, _, _| async move {
      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);
      let options = HeadOptions::new(headers);

      assert_eq!(storage.head("assets/key1", options).await.unwrap(), 6);
    })
    .await;
  }

  #[tokio::test]
  async fn format_url() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8080").unwrap(),
      vec!["*".to_string()],
      vec!["*".to_string()],
    );

    let mut headers = HeaderMap::default();
    let options = test_range_options(&mut headers);

    assert_eq!(
      storage.range_url("assets/key1", options).await.unwrap(),
      HtsGetUrl::new("https://localhost:8080/assets/key1")
        .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
    );
  }

  #[tokio::test]
  async fn format_url_different_response_scheme() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("http://example.com").unwrap(),
      vec!["*".to_string()],
      vec!["*".to_string()],
    );

    let mut headers = HeaderMap::default();
    let options = test_range_options(&mut headers);

    assert_eq!(
      storage.range_url("assets/key1", options).await.unwrap(),
      HtsGetUrl::new("http://example.com/assets/key1")
        .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
    );
  }

  #[tokio::test]
  async fn format_url_no_headers() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8081").unwrap(),
      vec![],
      vec![],
    );

    let mut headers = HeaderMap::default();
    let options = test_range_options(&mut headers);

    assert_eq!(
      storage.range_url("assets/key1", options).await.unwrap(),
      HtsGetUrl::new("https://localhost:8081/assets/key1")
    );
  }

  pub(crate) fn test_client() -> ClientWithMiddleware {
    reqwest_middleware::ClientBuilder::new(ClientBuilder::new().build().unwrap()).build()
  }

  pub(crate) async fn with_url_test_server<F, Fut>(test: F)
  where
    F: FnOnce(UrlStorage, String, PathBuf) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    with_test_server(base_path.path(), test).await;
  }

  pub(crate) async fn test_auth(
    request: Request<Body>,
    next: Next,
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
    F: FnOnce(UrlStorage, String, PathBuf) -> Fut,
    Fut: Future<Output = ()>,
  {
    let path = server_base_path.to_str().unwrap();
    let router = Router::new()
      .nest_service("/assets", ServeDir::new(path))
      .route_layer(middleware::from_fn(test_auth));

    // TODO fix this in htsget-test to bind and return tcp listener.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, router.into_make_service()).await });

    let url = format!("http://{addr}");
    test(
      UrlStorage::new(
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        vec!["*".to_string()],
        vec![],
      ),
      url,
      server_base_path.to_path_buf(),
    )
    .await;
  }

  pub(crate) fn test_headers(headers: &mut HeaderMap) -> &HeaderMap {
    headers.append(
      HeaderName::from_str(AUTHORIZATION.as_str()).unwrap(),
      HeaderValue::from_str("secret").unwrap(),
    );
    headers
  }

  pub(crate) fn test_range_options(headers: &mut HeaderMap) -> RangeUrlOptions<'_> {
    let headers = test_headers(headers);
    RangeUrlOptions::new_with_default_range(headers)
  }
}
