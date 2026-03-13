//! Defines the `JsonPath` storage backend.
//!

use std::fmt::{Debug, Display};
use std::str::FromStr;

use crate::StorageError::ResponseError;
use crate::url::{UrlClient, UrlStream};
use crate::{GetOptions, HeadOptions, RangeUrlOptions, Result, StorageMiddleware, StorageTrait};
use crate::{Streamable, Url as HtsGetUrl};
use async_trait::async_trait;
use htsget_config::config::advanced::json_path::JsonPathOrUrl;
use http::{HeaderMap, Method, Uri};
use jsonpath_rust::JsonPath;
use reqwest_middleware::ClientWithMiddleware;
use tracing::{debug, instrument};

/// A storage struct which derives data from an endpoint URL using json path.
#[derive(Debug, Clone)]
pub struct JsonPathStorage {
  url_client: UrlClient,
  resolve_from: Uri,
  content_path: String,
  size_path: Option<String>,
  response_path: Option<JsonPathOrUrl>,
}

impl JsonPathStorage {
  /// Construct a new `JsonPathStorage`.
  pub fn new(
    client: ClientWithMiddleware,
    resolve_from: Uri,
    content_path: String,
    size_path: Option<String>,
    response_path: Option<JsonPathOrUrl>,
    forward_headers: bool,
    header_blacklist: Vec<String>,
  ) -> Self {
    Self {
      url_client: UrlClient::new(client, forward_headers, header_blacklist),
      resolve_from,
      content_path,
      size_path,
      response_path,
    }
  }

  /// Get a url from the key.
  pub fn get_endpoint_url<K: AsRef<str>>(&self, key: K) -> Result<Uri> {
    self.url_client.append_key_to_url(&self.resolve_from, key)
  }

  /// Fetch the JSON data from the endpoint and query and parse the JSON path value.
  pub async fn resolve_endpoint<T, K>(&self, key: K, headers: HeaderMap, query: &str) -> Result<T>
  where
    K: AsRef<str>,
    T: FromStr,
    <T as FromStr>::Err: Display,
  {
    let endpoint_request = self.get_endpoint_url(key)?;

    let response = self
      .url_client
      .send_request(endpoint_request, Default::default(), headers, Method::GET)
      .await?
      .json::<serde_json::Value>()
      .await
      .map_err(|err| {
        ResponseError(format!(
          "deserializing body from {}: {}",
          self.resolve_from, err
        ))
      })?;

    // Get the queried value.
    let query_response = response.query(query).map_err(|err| {
      ResponseError(format!(
        "querying JSON path response from {}: {}",
        self.resolve_from, err
      ))
    })?;

    // First valid query result.
    let first_value = query_response.first().ok_or_else(|| {
      ResponseError(format!(
        "fetching single JSON value from {}",
        self.resolve_from
      ))
    })?;

    // Convert possible numbers to parsable strings in order to support a number or
    // a string from the response.
    let convert_to_string = first_value
      .as_u64()
      .or_else(|| first_value.as_i64().and_then(|n| u64::try_from(n).ok()))
      .map(|n| n.to_string())
      .or_else(|| first_value.as_str().map(|n| n.to_string()));

    // Parse the result.
    convert_to_string
      .ok_or_else(|| {
        ResponseError(format!(
          "path is not a string when fetching from {}",
          self.resolve_from
        ))
      })?
      .parse::<T>()
      .map_err(|err| {
        ResponseError(format!(
          "parsing content URL from {}: {}",
          self.resolve_from, err
        ))
      })
  }

  /// Get the size of the object from the key.
  pub async fn object_size<K: AsRef<str>>(&self, key: K, options: HeadOptions<'_>) -> Result<u64> {
    if let Some(ref size_path) = self.size_path {
      self
        .resolve_endpoint(key, options.request_headers().clone(), size_path)
        .await
    } else {
      let content_url = self
        .resolve_endpoint(
          key.as_ref(),
          options.request_headers().clone(),
          &self.content_path,
        )
        .await?;

      let response = self
        .url_client
        .send_request(
          content_url,
          Default::default(),
          options.request_headers().clone(),
          Method::HEAD,
        )
        .await?;

      UrlClient::extract_size(response)
    }
  }

  /// Get the key.
  pub async fn get_key<K: AsRef<str>>(
    &self,
    key: K,
    options: GetOptions<'_>,
  ) -> Result<reqwest::Response> {
    let content_url = self
      .resolve_endpoint(key, options.request_headers().clone(), &self.content_path)
      .await?;

    let headers = options.request_headers().clone();
    self
      .url_client
      .send_request(content_url, options.range, headers, Method::GET)
      .await
  }

  /// Format the response URL tickets.
  pub async fn format_key<K: AsRef<str>>(
    &self,
    key: K,
    options: RangeUrlOptions<'_>,
  ) -> Result<HtsGetUrl> {
    if let Some(ref response_path) = self.response_path {
      match response_path {
        JsonPathOrUrl::Url(url) => self
          .url_client
          .format_url(self.url_client.append_key_to_url(url, key)?, options),
        JsonPathOrUrl::JsonPath(response_path) => {
          let response_url = self
            .resolve_endpoint(
              key.as_ref(),
              options.response_headers().clone(),
              response_path,
            )
            .await?;

          self.url_client.format_url(response_url, options)
        }
      }
    } else {
      let content_url = self
        .resolve_endpoint(key, options.response_headers().clone(), &self.content_path)
        .await?;

      self.url_client.format_url(content_url, options)
    }
  }
}

#[async_trait]
impl StorageMiddleware for JsonPathStorage {}

#[async_trait]
impl StorageTrait for JsonPathStorage {
  #[instrument(level = "trace", skip(self))]
  async fn get(&self, key: &str, options: GetOptions<'_>) -> Result<Streamable> {
    debug!(calling_from = ?self, key, "getting url with key {:?}", key);

    let response = self.get_key(key.to_string(), options).await?;
    Ok(UrlStream::streamable_from_response(response))
  }

  #[instrument(level = "trace", skip(self))]
  async fn range_url(&self, key: &str, options: RangeUrlOptions<'_>) -> Result<HtsGetUrl> {
    debug!(calling_from = ?self, key, "formatting url with key {:?}", key);

    self.format_key(key, options).await
  }

  #[instrument(level = "trace", skip(self))]
  async fn head(&self, key: &str, options: HeadOptions<'_>) -> Result<u64> {
    debug!(calling_from = ?self, key, "getting head with key {:?}", key);

    let size = self.object_size(key, options).await?;

    debug!(calling_from = ?self, size, "size of key is {}", size);
    Ok(size)
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use axum::extract::Path as AxumPath;
  use axum::routing::get;
  use axum::{Json, Router, middleware};
  use htsget_config::types::Headers;
  use http::header::AUTHORIZATION;
  use serde_json::json;
  use std::future::Future;
  use std::path::{Path, PathBuf};
  use std::str::FromStr;
  use std::vec;
  use tokio::io::AsyncReadExt;
  use tokio::net::TcpListener;
  use tower_http::services::ServeDir;

  use super::*;
  use crate::local::tests::create_local_test_files;
  use crate::types::GetOptions;
  use crate::url::tests::{test_auth, test_client, test_headers, test_range_options};

  fn test_storage(uri: Uri) -> JsonPathStorage {
    JsonPathStorage::new(
      test_client(),
      uri,
      "$.content".to_string(),
      Some("$.size".to_string()),
      Some(JsonPathOrUrl::JsonPath("$.response".to_string())),
      true,
      vec![],
    )
  }

  #[test]
  fn get_endpoint_from_key() {
    let storage = test_storage("https://example.com".parse().unwrap());
    assert_eq!(
      storage.get_endpoint_url("assets/key1").unwrap(),
      Uri::from_str("https://example.com/assets/key1").unwrap()
    );
  }

  #[tokio::test]
  async fn get_key() {
    with_json_path_test_server(|storage, _, _| async move {
      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response = String::from_utf8(
        storage
          .get_key("key1", GetOptions::new_with_default_range(headers))
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
  async fn object_size() {
    with_json_path_test_server(|mut storage, _, _| async move {
      storage.size_path = None;
      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response = storage
        .object_size("key1", HeadOptions::new(headers))
        .await
        .unwrap();
      assert_eq!(response, 6);
    })
    .await;
  }

  #[tokio::test]
  async fn object_size_path_set() {
    with_json_path_test_server(test_object_size).await;
  }

  #[tokio::test]
  async fn object_size_path_set_text_size() {
    with_json_path_server_text_size(test_object_size).await;
  }

  #[tokio::test]
  async fn get_storage() {
    with_json_path_test_server(|storage, _, _| async move {
      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);
      let options = GetOptions::new_with_default_range(headers);

      let mut reader = storage.get("key1", options).await.unwrap();

      let mut response = [0; 6];
      reader.read_exact(&mut response).await.unwrap();

      assert_eq!(String::from_utf8(response.to_vec()).unwrap(), "value1");
    })
    .await;
  }

  #[tokio::test]
  async fn range_url_storage() {
    with_json_path_test_server(|mut storage, url, _| async move {
      storage.response_path = None;
      let mut headers = HeaderMap::default();
      let options = test_range_options(&mut headers);

      assert_eq!(
        storage.range_url("key1", options).await.unwrap(),
        HtsGetUrl::new(format!("{url}/assets/key1"))
          .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn head_storage() {
    with_json_path_test_server(|storage, _, _| async move {
      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);
      let options = HeadOptions::new(headers);

      assert_eq!(storage.head("key1", options).await.unwrap(), 3);
    })
    .await;
  }

  #[tokio::test]
  async fn format_key() {
    with_json_path_test_server(|mut storage, url, _| async move {
      storage.response_path = None;
      let mut headers = HeaderMap::default();
      let options = test_range_options(&mut headers);

      assert_eq!(
        storage.format_key("key1", options).await.unwrap(),
        HtsGetUrl::new(format!("{url}/assets/key1"))
          .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn format_key_path_set() {
    with_json_path_test_server(|storage, _, _| async move {
      let mut headers = HeaderMap::default();
      let options = test_range_options(&mut headers);

      test_format_key(storage, options).await;
    })
    .await;
  }

  #[tokio::test]
  async fn format_key_no_headers() {
    with_json_path_test_server(|mut storage, _, _| async move {
      storage.url_client = UrlClient::new(test_client(), false, vec![]);
      let mut headers = HeaderMap::default();
      let options = test_range_options(&mut headers);

      assert_eq!(
        storage.format_key("key1", options).await.unwrap(),
        HtsGetUrl::new("https://example.com/key1".to_string())
      );
    })
    .await;
  }

  #[tokio::test]
  async fn format_key_url_response_path() {
    with_json_path_test_server(|mut storage, _, _| async move {
      storage.response_path = Some(JsonPathOrUrl::Url("https://example.com".parse().unwrap()));
      let mut headers = HeaderMap::default();
      let options = test_range_options(&mut headers);

      test_format_key(storage, options).await;
    })
    .await;
  }

  async fn test_format_key(storage: JsonPathStorage, options: RangeUrlOptions<'_>) {
    assert_eq!(
      storage.format_key("key1", options).await.unwrap(),
      HtsGetUrl::new("https://example.com/key1".to_string())
        .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
    );
  }

  async fn test_object_size(storage: JsonPathStorage, _url: String, _path: PathBuf) {
    let mut headers = HeaderMap::default();
    let headers = test_headers(&mut headers);

    let response = storage
      .object_size("key1", HeadOptions::new(headers))
      .await
      .unwrap();
    assert_eq!(response, 3);
  }

  pub(crate) async fn with_json_path_test_server<F, Fut>(test: F)
  where
    F: FnOnce(JsonPathStorage, String, PathBuf) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    with_test_server(base_path.path(), test).await;
  }

  pub(crate) async fn with_json_path_server_text_size<F, Fut>(test: F)
  where
    F: FnOnce(JsonPathStorage, String, PathBuf) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    with_test_server_text_size(base_path.path(), test).await;
  }

  async fn with_test_server_impl<F, Fut>(server_base_path: &Path, test: F, size: serde_json::Value)
  where
    F: FnOnce(JsonPathStorage, String, PathBuf) -> Fut,
    Fut: Future<Output = ()>,
  {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    let url_clone = url.clone();

    let path = server_base_path.to_str().unwrap();
    let router = Router::new()
      .nest_service("/assets", ServeDir::new(path))
      .route(
        "/{id}",
        get(|AxumPath(id): AxumPath<String>| async move {
          Json(json!({
            "content": format!("{url_clone}/assets/{id}"),
            "size": size,
            "response": format!("https://example.com/{id}")
          }))
        }),
      )
      .route_layer(middleware::from_fn(test_auth));

    tokio::spawn(async move { axum::serve(listener, router.into_make_service()).await });

    test(
      test_storage(url.parse().unwrap()),
      url,
      server_base_path.to_path_buf(),
    )
    .await;
  }

  pub(crate) async fn with_test_server<F, Fut>(server_base_path: &Path, test: F)
  where
    F: FnOnce(JsonPathStorage, String, PathBuf) -> Fut,
    Fut: Future<Output = ()>,
  {
    with_test_server_impl(
      server_base_path,
      test,
      serde_json::Value::Number(serde_json::Number::from_u128(3).unwrap()),
    )
    .await;
  }

  pub(crate) async fn with_test_server_text_size<F, Fut>(server_base_path: &Path, test: F)
  where
    F: FnOnce(JsonPathStorage, String, PathBuf) -> Fut,
    Fut: Future<Output = ()>,
  {
    with_test_server_impl(
      server_base_path,
      test,
      serde_json::Value::String("3".to_string()),
    )
    .await;
  }
}
