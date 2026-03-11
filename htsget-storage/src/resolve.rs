//! Defines the `ResolveStorage` storage backend.
//!

use std::fmt::{Debug, Display};
use std::str::FromStr;

use async_trait::async_trait;
use http::{HeaderMap, Method, Uri};
use jsonpath_rust::JsonPath;
use reqwest_middleware::ClientWithMiddleware;
use tracing::{debug, instrument};

use crate::StorageError::{ResponseError, UrlParseError};
use crate::url::{UrlClient, UrlStream};
use crate::{GetOptions, HeadOptions, RangeUrlOptions, Result, StorageMiddleware, StorageTrait};
use crate::{Streamable, Url as HtsGetUrl};

/// A storage struct which derives data from a resolve endpoint URL.
#[derive(Debug, Clone)]
pub struct ResolveStorage {
  url_client: UrlClient,
  resolve_from: Uri,
  content_path: String,
  size_path: Option<String>,
  response_path: Option<String>,
}

impl ResolveStorage {
  /// Construct a new `ResolveStorage`.
  pub fn new(
    client: ClientWithMiddleware,
    resolve_from: Uri,
    content_path: String,
    size_path: Option<String>,
    response_path: Option<String>,
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
    format!("{}{}", self.resolve_from, key.as_ref())
      .parse::<Uri>()
      .map_err(|err| UrlParseError(err.to_string()))
  }

  /// Fetch the JSON data from the resolve endpoint and query and parse the JSON path value.
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

    response
      .query(query)
      .map_err(|err| {
        ResponseError(format!(
          "querying JSON path response from {}: {}",
          self.resolve_from, err
        ))
      })?
      .first()
      .ok_or_else(|| {
        ResponseError(format!(
          "fetching single JSON value from {}",
          self.resolve_from
        ))
      })?
      .as_str()
      .ok_or_else(|| {
        ResponseError(format!(
          "content path is not a string when resolving from {}",
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

    self
      .url_client
      .send_request(
        content_url,
        Default::default(),
        options.request_headers().clone(),
        Method::GET,
      )
      .await
  }

  /// Format the response URL tickets.
  pub async fn format_key<K: AsRef<str>>(
    &self,
    key: K,
    options: RangeUrlOptions<'_>,
  ) -> Result<HtsGetUrl> {
    if let Some(ref response_path) = self.response_path {
      let response_url = self
        .resolve_endpoint(key, options.response_headers().clone(), response_path)
        .await?;
      self.url_client.format_url(response_url, options)
    } else {
      self
        .url_client
        .format_url(self.get_endpoint_url(key)?, options)
    }
  }
}

#[async_trait]
impl StorageMiddleware for ResolveStorage {}

#[async_trait]
impl StorageTrait for ResolveStorage {
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
