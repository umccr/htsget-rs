//! The config for json path locations.
//!

use crate::config::advanced::HttpClient;
use crate::error::Error;
use crate::error::Error::ParseError;
use crate::error::Result;
use crate::http::client::HttpClientConfig;
use crate::storage;
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use crate::storage::json_path::JsonPathOrUrl;
use cfg_if::cfg_if;
use http::uri::InvalidUri;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Options for getting config data from a remote endpoint using json path.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct JsonPath {
  resolve_from: String,
  content_path: String,
  size_path: Option<String>,
  response_path: Option<String>,
  response_url: Option<String>,
  allow_headers_backend: Vec<String>,
  deny_headers_backend: Vec<String>,
  allow_headers_client: Vec<String>,
  deny_headers_client: Vec<String>,
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
    resolve_from: String,
    content_path: String,
    size_path: Option<String>,
    response_path: Option<String>,
    response_url: Option<String>,
    allow_headers_backend: Vec<String>,
    allow_headers_client: Vec<String>,
  ) -> Self {
    Self {
      resolve_from,
      content_path,
      size_path,
      response_path,
      response_url,
      allow_headers_backend,
      deny_headers_backend: vec![],
      allow_headers_client,
      deny_headers_client: vec![],
      http: HttpClientConfig::default(),
      #[cfg(feature = "experimental")]
      keys: None,
      is_defaulted: false,
      #[cfg(feature = "experimental")]
      forward_public_key: true,
    }
  }

  /// Set the headers blocked from forwarding to the backend server.
  pub fn set_deny_headers_backend(&mut self, deny_headers_backend: Vec<String>) {
    self.deny_headers_backend = deny_headers_backend;
  }

  /// Set the headers blocked from being reflected back to the client.
  pub fn set_deny_headers_client(&mut self, deny_headers_client: Vec<String>) {
    self.deny_headers_client = deny_headers_client;
  }

  /// Get the resolve API url.
  pub fn resolve_from(&self) -> &str {
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
  pub fn response_path(&self) -> Option<&str> {
    self.response_path.as_deref()
  }

  /// Get the response url.
  pub fn response_url(&self) -> Option<&str> {
    self.response_url.as_deref()
  }

  /// Get the headers forwarded to the backend storage server. Supports wildcards using `*` and `?`.
  pub fn allow_headers_backend(&self) -> &[String] {
    &self.allow_headers_backend
  }

  /// Get the headers blocked from being forwarded to the backend storage server. Supports wildcards using `*` and `?`.
  pub fn deny_headers_backend(&self) -> &[String] {
    &self.deny_headers_backend
  }

  /// Get the headers reflected back to the client in tickets. Supports wildcards using `*` and `?`.
  pub fn allow_headers_client(&self) -> &[String] {
    &self.allow_headers_client
  }

  /// Get the headers blocked from being reflected back to the client in tickets. Supports wildcards using `*` and `?`.
  pub fn deny_headers_client(&self) -> &[String] {
    &self.deny_headers_client
  }

  /// Get the http client config.
  pub fn http(&self) -> &HttpClientConfig {
    &self.http
  }

  /// Set the http client config.
  pub fn set_http(&mut self, http: HttpClientConfig) {
    self.http = http;
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

    let map_err = |err: InvalidUri| ParseError(err.to_string());
    let response_url = match (storage.response_path, storage.response_url) {
      (None, None) => None,
      (Some(path), None) => Some(JsonPathOrUrl::JsonPath(path)),
      (None, Some(url)) => Some(JsonPathOrUrl::Url(url.parse().map_err(map_err)?)),
      (Some(_), Some(_)) => {
        return Err(ParseError(
          "cannot set both a `response_path` and `response_url`".to_string(),
        ));
      }
    };

    let mut url_storage = Self::new(
      storage.resolve_from.parse().map_err(map_err)?,
      storage.content_path,
      storage.size_path,
      response_url,
      storage.allow_headers_backend,
      storage.allow_headers_client,
      client,
    );
    url_storage.set_deny_headers_backend(storage.deny_headers_backend);
    url_storage.set_deny_headers_client(storage.deny_headers_client);

    cfg_if! {
      if #[cfg(feature = "experimental")] {
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
      Default::default(),
      vec!["*".to_string()],
      vec!["*".to_string()],
    );

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
      allow_headers_backend = ["Authorization"]
      deny_headers_backend = ["X-Internal-*"]
      allow_headers_client = ["Authorization"]
      deny_headers_client = ["X-Internal-*"]
      "#,
      Ok((
        "https://example.com/".to_string(),
        Some(JsonPathOrUrl::JsonPath("$.response".to_string())),
        "$.content".to_string(),
        Some("$.size".to_string()),
        vec!["Authorization".to_string()],
        vec!["X-Internal-*".to_string()],
        vec!["Authorization".to_string()],
        vec!["X-Internal-*".to_string()],
      )),
      get_result_values,
    );
  }

  #[test]
  fn json_path_backend_url_response() {
    test_serialize_and_deserialize(
      r#"
      resolve_from = "https://example.com"
      response_url = "https://example.com"
      content_path = "$.content"
      size_path = "$.size"
      allow_headers_backend = ["Authorization"]
      deny_headers_backend = ["X-Internal-*"]
      allow_headers_client = ["Authorization"]
      deny_headers_client = ["X-Internal-*"]
      "#,
      Ok((
        "https://example.com/".to_string(),
        Some(JsonPathOrUrl::Url("https://example.com".parse().unwrap())),
        "$.content".to_string(),
        Some("$.size".to_string()),
        vec!["Authorization".to_string()],
        vec!["X-Internal-*".to_string()],
        vec!["Authorization".to_string()],
        vec!["X-Internal-*".to_string()],
      )),
      get_result_values,
    );
  }

  #[test]
  fn json_path_backend_url_and_path_err() {
    test_serialize_and_deserialize(
      r#"
      resolve_from = "https://example.com"
      response_url = "https://example.com"
      response_path = "$.response"
      content_path = "$.content"
      size_path = "$.size"
      allow_headers_backend = ["Authorization"]
      deny_headers_backend = ["X-Internal-*"]
      allow_headers_client = ["Authorization"]
      deny_headers_client = ["X-Internal-*"]
      "#,
      (),
      |result| {
        let value = get_result_values(result);
        assert!(value.is_err());
      },
    );
  }

  type JsonPathResultValues = Result<(
    String,
    Option<JsonPathOrUrl>,
    String,
    Option<String>,
    Vec<String>,
    Vec<String>,
    Vec<String>,
    Vec<String>,
  )>;

  fn get_result_values(result: JsonPath) -> JsonPathResultValues {
    let result = storage::json_path::JsonPath::try_from(result)?;
    Ok((
      result.resolve_from().to_string(),
      result.response_path().cloned(),
      result.content_path().to_string(),
      result.size_path().map(|value| value.to_string()),
      result.allow_headers_backend().to_vec(),
      result.deny_headers_backend().to_vec(),
      result.allow_headers_client().to_vec(),
      result.deny_headers_client().to_vec(),
    ))
  }
}
