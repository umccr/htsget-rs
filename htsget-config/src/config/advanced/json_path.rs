//! The config for json path locations.
//!

use crate::config::advanced::HttpClient;
use crate::error::Error;
use crate::error::Result;
use crate::http::client::HttpClientConfig;
use crate::storage;
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use cfg_if::cfg_if;
use http::Uri;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt::Display;
use std::{fmt, result};

/// Either a JSON path or a url.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonPathOrUrl {
  Url(Uri),
  JsonPath(String),
}

impl Display for JsonPathOrUrl {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s = match self {
      JsonPathOrUrl::Url(url) => &url.to_string(),
      JsonPathOrUrl::JsonPath(url) => url.as_str(),
    };
    f.write_str(s)
  }
}

impl Default for JsonPathOrUrl {
  fn default() -> Self {
    Self::JsonPath("$".to_string())
  }
}

impl<'de> Deserialize<'de> for JsonPathOrUrl {
  fn deserialize<D>(deserializer: D) -> result::Result<JsonPathOrUrl, D::Error>
  where
    D: Deserializer<'de>,
  {
    let url_or_json_path = String::deserialize(deserializer)?;

    if url_or_json_path.starts_with("$") {
      Ok(JsonPathOrUrl::JsonPath(url_or_json_path))
    } else {
      Ok(JsonPathOrUrl::Url(
        url_or_json_path.parse().map_err(de::Error::custom)?,
      ))
    }
  }
}

impl Serialize for JsonPathOrUrl {
  fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    self.to_string().serialize(serializer)
  }
}

/// Options for getting config data from a remote endpoint using json path.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct JsonPath {
  #[schemars(with = "String")]
  #[serde(with = "http_serde::uri")]
  resolve_from: Uri,
  content_path: String,
  size_path: Option<String>,
  #[schemars(with = "Option<String>")]
  response_path: Option<JsonPathOrUrl>,
  forward_headers: bool,
  header_blacklist: Vec<String>,
  #[schemars(skip)]
  #[serde(alias = "tls", skip_serializing)]
  http: HttpClientConfig,
  #[cfg(feature = "experimental")]
  #[serde(skip_serializing)]
  keys: Option<C4GHKeys>,
  #[cfg(feature = "experimental")]
  forward_public_key: bool,
  #[serde(skip)]
  pub(crate) is_defaulted: bool,
}

impl JsonPath {
  /// Create a new json path storage.
  pub fn new(
    resolve_from: Uri,
    content_path: String,
    size_path: Option<String>,
    response_path: Option<JsonPathOrUrl>,
    forward_headers: bool,
    header_blacklist: Vec<String>,
    http: HttpClientConfig,
  ) -> Self {
    Self {
      resolve_from,
      content_path,
      size_path,
      response_path,
      forward_headers,
      header_blacklist,
      http,
      #[cfg(feature = "experimental")]
      keys: None,
      is_defaulted: false,
      #[cfg(feature = "experimental")]
      forward_public_key: true,
    }
  }

  /// Get the resolve API url.
  pub fn resolve_from(&self) -> &Uri {
    &self.resolve_from
  }

  /// Get the content path that controls where in the response to get content from.
  pub fn content_path(&self) -> &str {
    &self.content_path
  }

  /// Get the content path that controls where in the response to get the size of the object from.
  pub fn size_path(&self) -> Option<&str> {
    self.size_path.as_deref()
  }

  /// Get the response path.
  pub fn response_path(&self) -> Option<&JsonPathOrUrl> {
    self.response_path.as_ref()
  }

  /// Whether headers received in a query request should be
  /// included in the returned data block tickets.
  pub fn forward_headers(&self) -> bool {
    self.forward_headers
  }

  /// Get the http client config.
  pub fn http(&self) -> &HttpClientConfig {
    &self.http
  }

  /// Set the C4GH keys.
  #[cfg(feature = "experimental")]
  pub fn set_keys(mut self, keys: Option<C4GHKeys>) -> Self {
    self.keys = keys;
    self
  }

  /// Get the C4GH keys.
  #[cfg(feature = "experimental")]
  pub fn keys(&self) -> Option<&C4GHKeys> {
    self.keys.as_ref()
  }

  /// Set whether to forward the public key in a context header.
  #[cfg(feature = "experimental")]
  pub fn set_forward_public_key(&mut self, forward_public_key: bool) {
    self.forward_public_key = forward_public_key;
  }

  /// Whether to forward the public key in a context header.
  #[cfg(feature = "experimental")]
  pub fn forward_public_key(&self) -> bool {
    self.forward_public_key
  }
}

impl TryFrom<JsonPath> for storage::json_path::JsonPath {
  type Error = Error;

  fn try_from(storage: JsonPath) -> Result<Self> {
    let client = HttpClient::from(storage.http);

    let url_storage = Self::new(
      storage.resolve_from.clone(),
      storage.content_path,
      storage.size_path,
      storage.response_path,
      storage.forward_headers,
      storage.header_blacklist,
      client,
    );

    cfg_if! {
      if #[cfg(feature = "experimental")] {
        let mut url_storage = url_storage;
        url_storage.set_keys(storage.keys);
        url_storage.set_forward_public_key(storage.forward_public_key);
        Ok(url_storage)
      } else {
        Ok(url_storage)
      }
    }
  }
}

impl Default for JsonPath {
  fn default() -> Self {
    let mut url = Self::new(
      Default::default(),
      Default::default(),
      Default::default(),
      Default::default(),
      true,
      Default::default(),
      Default::default(),
    );

    #[cfg(feature = "experimental")]
    {
      url.set_forward_public_key(true);
    }

    url.is_defaulted = true;
    url
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::tests::test_serialize_and_deserialize;

  #[test]
  fn json_path_backend() {
    test_serialize_and_deserialize(
      r#"
      resolve_from = "https://example.com"
      response_path = "$.response"
      content_path = "$.content"
      size_path = "$.size"
      forward_headers = false
      header_blacklist = ["Host"]
      "#,
      (
        "https://example.com/".to_string(),
        Some(JsonPathOrUrl::JsonPath("$.response".to_string())),
        "$.content".to_string(),
        Some("$.size".to_string()),
        false,
        vec!["Host".to_string()],
      ),
      get_result_values,
    );
  }

  #[test]
  fn json_path_backend_url_response() {
    test_serialize_and_deserialize(
      r#"
      resolve_from = "https://example.com"
      response_path = "https://example.com"
      content_path = "$.content"
      size_path = "$.size"
      forward_headers = false
      header_blacklist = ["Host"]
      "#,
      (
        "https://example.com/".to_string(),
        Some(JsonPathOrUrl::Url("https://example.com".parse().unwrap())),
        "$.content".to_string(),
        Some("$.size".to_string()),
        false,
        vec!["Host".to_string()],
      ),
      get_result_values,
    );
  }

  fn get_result_values(
    result: JsonPath,
  ) -> (
    String,
    Option<JsonPathOrUrl>,
    String,
    Option<String>,
    bool,
    Vec<String>,
  ) {
    (
      result.resolve_from().to_string(),
      result.response_path().cloned(),
      result.content_path().to_string(),
      result.size_path().map(|value| value.to_string()),
      result.forward_headers(),
      result.header_blacklist,
    )
  }
}
