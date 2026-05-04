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
  allow_headers_backend: Vec<WildMatch>,
  deny_headers_backend: Vec<WildMatch>,
  allow_headers_client: Vec<WildMatch>,
  deny_headers_client: Vec<WildMatch>,
}

impl UrlClient {
  /// Construct a new `UrlClient`.
  pub fn new(
    client: ClientWithMiddleware,
    allow_headers_backend: Vec<String>,
    deny_headers_backend: Vec<String>,
    allow_headers_client: Vec<String>,
    deny_headers_client: Vec<String>,
  ) -> Self {
    Self {
      client,
      allow_headers_backend: Self::map_filters(allow_headers_backend),
      deny_headers_backend: Self::map_filters(deny_headers_backend),
      allow_headers_client: Self::map_filters(allow_headers_client),
      deny_headers_client: Self::map_filters(deny_headers_client),
    }
  }

  /// Compile filter patterns, ensure case insensitivity of headers is preserved.
  fn map_filters(filters: Vec<String>) -> Vec<WildMatch> {
    filters
      .iter()
      .map(|f| WildMatch::new(&f.to_lowercase()))
      .collect()
  }

  /// Filter a header map, keeping only headers whose names match at least one of the allow
  /// patterns and do not match any of the deny patterns.
  fn filter_headers(headers: HeaderMap, allow: &[WildMatch], deny: &[WildMatch]) -> HeaderMap {
    if allow.is_empty() {
      return HeaderMap::new();
    }

    let mut keep = false;
    let mut result = HeaderMap::new();
    result.extend(headers.into_iter().filter_map(|(name, value)| {
      if let Some(n) = name.as_ref() {
        let name_str = n.as_str();
        keep = allow.iter().any(|pattern| pattern.matches(name_str))
          && !deny.iter().any(|pattern| pattern.matches(name_str));
      }
      keep.then_some((name, value))
    }));
    result
  }

  /// Set the headers blocked from being forwarded to the backend storage server.
  pub fn set_deny_headers_backend(&mut self, deny_headers_backend: Vec<String>) {
    self.deny_headers_backend = Self::map_filters(deny_headers_backend);
  }

  /// Set the headers blocked from being reflected back to the client in tickets.
  pub fn set_deny_headers_client(&mut self, deny_headers_client: Vec<String>) {
    self.deny_headers_client = Self::map_filters(deny_headers_client);
  }

  /// Filter headers to only those matching the `allow_headers_backend` patterns and not matching
  /// the `deny_headers_backend` patterns.
  pub fn filter_forward_headers(&self, headers: HeaderMap) -> HeaderMap {
    Self::filter_headers(
      headers,
      &self.allow_headers_backend,
      &self.deny_headers_backend,
    )
  }

  /// Filter headers to only those matching the `allow_headers_client` patterns and not matching
  /// the `deny_headers_client` patterns.
  pub fn filter_reflect_headers(&self, headers: HeaderMap) -> HeaderMap {
    Self::filter_headers(
      headers,
      &self.allow_headers_client,
      &self.deny_headers_client,
    )
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
    allow_headers_backend: Vec<String>,
    deny_headers_backend: Vec<String>,
    allow_headers_client: Vec<String>,
    deny_headers_client: Vec<String>,
  ) -> Self {
    Self {
      url,
      response_url,
      url_client: UrlClient::new(
        client,
        allow_headers_backend,
        deny_headers_backend,
        allow_headers_client,
        deny_headers_client,
      ),
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
  use axum::extract::State;
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
      vec![],
      vec!["*".to_string()],
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
      vec!["*".to_string()],
      vec![],
      vec!["*".to_string()],
      vec![],
    );

    assert_eq!(
      storage.get_response_url_from_key("assets/key1").unwrap(),
      Uri::from_str("https://localhost:8080/assets/key1").unwrap()
    );
  }

  #[test]
  fn filter_wildcard_headers() {
    let storage = UrlClient::new(
      test_client(),
      vec!["authorization".to_string()],
      vec![],
      vec!["*".to_string()],
      vec![],
    );
    let mut headers = HeaderMap::default();
    headers.insert(
      HeaderName::from_str(HOST.as_str()).unwrap(),
      HeaderValue::from_str("example.com").unwrap(),
    );
    let result = storage.filter_forward_headers(headers.clone());
    assert!(result.is_empty());
    let result = storage.filter_reflect_headers(headers.clone());
    assert_eq!(result.len(), 1);
    assert!(result.get(HOST).is_some());

    headers.insert(
      HeaderName::from_str(AUTHORIZATION.as_str()).unwrap(),
      HeaderValue::from_str("secret").unwrap(),
    );
    let result = storage.filter_forward_headers(headers.clone());
    assert_eq!(result.len(), 1);
    assert!(result.get(AUTHORIZATION).is_some());
    let result = storage.filter_reflect_headers(headers.clone());
    assert_eq!(result.len(), 2);
    assert!(result.get(AUTHORIZATION).is_some());
    assert!(result.get(HOST).is_some());

    let storage = UrlClient::new(
      test_client(),
      vec!["auth*".to_string()],
      vec![],
      vec!["hos?".to_string()],
      vec![],
    );
    let mut headers = HeaderMap::default();
    headers.insert(HOST, HeaderValue::from_str("example.com").unwrap());
    let result = storage.filter_forward_headers(headers.clone());
    assert!(result.is_empty());
    let result = storage.filter_reflect_headers(headers.clone());
    assert_eq!(result.len(), 1);
    assert!(result.get(HOST).is_some());

    headers.insert(AUTHORIZATION, HeaderValue::from_str("secret").unwrap());
    let result = storage.filter_forward_headers(headers.clone());
    assert_eq!(result.len(), 1);
    assert!(result.get(AUTHORIZATION).is_some());
    let result = storage.filter_reflect_headers(headers.clone());
    assert_eq!(result.len(), 1);
    assert!(result.get(HOST).is_some());
  }

  #[test]
  fn filter_denylist_headers() {
    let storage = UrlClient::new(
      test_client(),
      vec!["*".to_string()],
      vec!["authorization".to_string()],
      vec!["*".to_string()],
      vec!["x-internal-*".to_string()],
    );

    let mut headers = HeaderMap::default();
    headers.insert(HOST, HeaderValue::from_str("example.com").unwrap());
    headers.insert(AUTHORIZATION, HeaderValue::from_str("secret").unwrap());
    headers.insert(
      HeaderName::from_str("x-internal-trace").unwrap(),
      HeaderValue::from_str("trace").unwrap(),
    );

    let result = storage.filter_forward_headers(headers.clone());
    assert_eq!(result.len(), 2);
    assert!(result.get(AUTHORIZATION).is_none());
    assert!(result.get(HOST).is_some());
    assert!(result.get("x-internal-trace").is_some());

    let result = storage.filter_reflect_headers(headers.clone());
    assert_eq!(result.len(), 2);
    assert!(result.get(AUTHORIZATION).is_some());
    assert!(result.get(HOST).is_some());
    assert!(result.get("x-internal-trace").is_none());

    let storage = UrlClient::new(
      test_client(),
      vec!["authorization".to_string()],
      vec!["authorization".to_string()],
      vec!["*".to_string()],
      vec!["*".to_string()],
    );
    let result = storage.filter_forward_headers(headers.clone());
    assert!(result.is_empty());
    let result = storage.filter_reflect_headers(headers);
    assert!(result.is_empty());
  }

  #[test]
  fn filter_mixed_case_patterns() {
    let storage = UrlClient::new(
      test_client(),
      vec!["Authorization".to_string(), "X-Custom-*".to_string()],
      vec!["X-Internal-*".to_string()],
      vec!["*".to_string()],
      vec!["AUTHORIZATION".to_string()],
    );

    let mut headers = HeaderMap::default();
    headers.insert(HOST, HeaderValue::from_str("example.com").unwrap());
    headers.insert(AUTHORIZATION, HeaderValue::from_str("secret").unwrap());
    headers.insert(
      HeaderName::from_str("x-custom-trace").unwrap(),
      HeaderValue::from_str("trace").unwrap(),
    );
    headers.insert(
      HeaderName::from_str("x-internal-debug").unwrap(),
      HeaderValue::from_str("debug").unwrap(),
    );

    let result = storage.filter_forward_headers(headers.clone());
    assert_eq!(result.len(), 2);
    assert!(result.get(AUTHORIZATION).is_some());
    assert!(result.get("x-custom-trace").is_some());
    assert!(result.get(HOST).is_none());
    assert!(result.get("x-internal-debug").is_none());

    let result = storage.filter_reflect_headers(headers);
    assert_eq!(result.len(), 3);
    assert!(result.get(AUTHORIZATION).is_none());
    assert!(result.get(HOST).is_some());
    assert!(result.get("x-custom-trace").is_some());
    assert!(result.get("x-internal-debug").is_some());
  }

  #[test]
  fn filter_repeated_header() {
    let storage = UrlClient::new(
      test_client(),
      vec!["accept".to_string()],
      vec![],
      vec!["x-internal".to_string()],
      vec![],
    );

    let mut headers = HeaderMap::default();
    headers.append(
      HeaderName::from_str("accept").unwrap(),
      HeaderValue::from_str("text/html").unwrap(),
    );
    headers.append(
      HeaderName::from_str("accept").unwrap(),
      HeaderValue::from_str("application/json").unwrap(),
    );
    headers.append(HOST, HeaderValue::from_str("example.com").unwrap());
    headers.append(
      HeaderName::from_str("x-internal").unwrap(),
      HeaderValue::from_str("a").unwrap(),
    );
    headers.append(
      HeaderName::from_str("x-internal").unwrap(),
      HeaderValue::from_str("b").unwrap(),
    );

    let result = storage.filter_forward_headers(headers.clone());
    let accept_values: Vec<_> = result
      .get_all("accept")
      .iter()
      .map(|v| v.to_str().unwrap().to_string())
      .collect();
    assert_eq!(accept_values, vec!["text/html", "application/json"]);
    assert_eq!(result.get_all("x-internal").iter().count(), 0);
    assert!(result.get(HOST).is_none());

    let result = storage.filter_reflect_headers(headers);
    let cookie_values: Vec<_> = result
      .get_all("x-internal")
      .iter()
      .map(|v| v.to_str().unwrap().to_string())
      .collect();
    assert_eq!(cookie_values, vec!["a", "b"]);
    assert_eq!(result.get_all("accept").iter().count(), 0);
  }

  #[tokio::test]
  async fn send_request_filter_headers() {
    // The test server middleware asserts that the headers reach the backend.
    for forward in [WildMatch::new("authorization"), WildMatch::new("auth*")] {
      with_url_test_server(
        |mut storage, _, _| async move {
          let mut headers = HeaderMap::default();
          headers.insert(HOST, HeaderValue::from_str("example.com").unwrap());
          let headers = test_headers(&mut headers);

          storage.url_client.allow_headers_backend = vec![forward];

          storage
            .url_client
            .send_request(
              storage.get_url_from_key("assets/key1").unwrap(),
              Default::default(),
              headers.clone(),
              Method::GET,
            )
            .await
            .unwrap();
        },
        vec![AUTHORIZATION],
      )
      .await;
    }
  }

  #[tokio::test]
  async fn send_request() {
    with_url_test_server(
      |storage, _, _| async move {
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
      },
      vec![],
    )
    .await;
  }

  #[tokio::test]
  async fn get_key() {
    with_url_test_server(
      |storage, _, _| async move {
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
      },
      vec![],
    )
    .await;
  }

  #[tokio::test]
  async fn head_key() {
    with_url_test_server(
      |storage, _, _| async move {
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
      },
      vec![],
    )
    .await;
  }

  #[tokio::test]
  async fn get_storage() {
    with_url_test_server(
      |storage, _, _| async move {
        let mut headers = HeaderMap::default();
        let headers = test_headers(&mut headers);
        let options = GetOptions::new_with_default_range(headers);

        let mut reader = storage.get("assets/key1", options).await.unwrap();

        let mut response = [0; 6];
        reader.read_exact(&mut response).await.unwrap();

        assert_eq!(String::from_utf8(response.to_vec()).unwrap(), "value1");
      },
      vec![],
    )
    .await;
  }

  #[tokio::test]
  async fn range_url_storage() {
    with_url_test_server(
      |_, url, _| async move {
        let storage = UrlStorage::new(
          test_client(),
          Uri::from_str(&url).unwrap(),
          Uri::from_str(&url).unwrap(),
          vec!["*".to_string()],
          vec![],
          vec!["*".to_string()],
          vec![],
        );
        let mut headers = HeaderMap::default();
        let options = test_range_options(&mut headers);

        assert_eq!(
          storage.range_url("assets/key1", options).await.unwrap(),
          HtsGetUrl::new(format!("{url}/assets/key1"))
            .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
        );
      },
      vec![],
    )
    .await;
  }

  #[tokio::test]
  async fn range_url_storage_filtered_headers() {
    with_url_test_server(
      |_, url, _| async move {
        let storage = UrlStorage::new(
          test_client(),
          Uri::from_str(&url).unwrap(),
          Uri::from_str(&url).unwrap(),
          vec!["*".to_string()],
          vec![],
          vec!["authorization".to_string()],
          vec![],
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
      },
      vec![],
    )
    .await;
  }

  #[tokio::test]
  async fn head_storage() {
    with_url_test_server(
      |storage, _, _| async move {
        let mut headers = HeaderMap::default();
        let headers = test_headers(&mut headers);
        let options = HeadOptions::new(headers);

        assert_eq!(storage.head("assets/key1", options).await.unwrap(), 6);
      },
      vec![],
    )
    .await;
  }

  #[tokio::test]
  async fn format_url() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8080").unwrap(),
      vec!["*".to_string()],
      vec![],
      vec!["*".to_string()],
      vec![],
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
      vec![],
      vec!["*".to_string()],
      vec![],
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

  pub(crate) async fn with_url_test_server<F, Fut>(test: F, expected_headers: Vec<HeaderName>)
  where
    F: FnOnce(UrlStorage, String, PathBuf) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    with_test_server(base_path.path(), test, expected_headers).await;
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

  pub(crate) async fn with_test_server<F, Fut>(
    server_base_path: &Path,
    test: F,
    expected_headers: Vec<HeaderName>,
  ) where
    F: FnOnce(UrlStorage, String, PathBuf) -> Fut,
    Fut: Future<Output = ()>,
  {
    let path = server_base_path.to_str().unwrap();
    let router = Router::new()
      .nest_service("/assets", ServeDir::new(path))
      .route_layer(middleware::from_fn_with_state(
        expected_headers,
        |State(headers): State<Vec<HeaderName>>, req: Request<Body>, next: Next| async move {
          if !headers.is_empty() {
            let mut req_headers = req
              .headers()
              .keys()
              .map(|h| h.as_str().to_string())
              .filter(|h| !&["host", "accept"].contains(&h.as_str()))
              .collect::<Vec<_>>();
            req_headers.sort();
            let mut expected_headers = headers
              .into_iter()
              .map(|h| h.as_str().to_string())
              .collect::<Vec<_>>();
            expected_headers.sort();

            assert_eq!(req_headers, expected_headers);
          }

          next.run(req).await
        },
      ))
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
        vec![],
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
