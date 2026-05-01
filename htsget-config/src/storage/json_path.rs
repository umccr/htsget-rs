//! Configuration for the Resolver storage type.
//!

use crate::config::advanced;
use crate::config::advanced::HttpClient;
use crate::error::Result;
use crate::http::client::HttpClientConfig;
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use http::Uri;
use reqwest_middleware::ClientWithMiddleware;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fmt::Display;

/// Either a JSON path or a url.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JsonPathOrUrl {
  Url(#[serde(with = "http_serde::uri")] Uri),
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

/// Configure the server to resolve endpoints from a Url using json path.
#[derive(JsonSchema, Deserialize, Serialize, Debug, Clone)]
#[serde(try_from = "advanced::json_path::JsonPath", deny_unknown_fields)]
pub struct JsonPath {
  /// The URL to resolve from.
  #[schemars(with = "String")]
  #[serde(with = "http_serde::uri")]
  resolve_from: Uri,
  /// The json path pointing to a url to fetch data from.
  content_path: String,
  /// The json path pointing to the size of the object. This avoids an additional head call on the
  /// content path url.
  size_path: Option<String>,
  /// The json path for the response tickets.
  #[schemars(with = "Option<String>")]
  response_path: Option<JsonPathOrUrl>,
  /// Headers that are forwarded to the backend storage server. Supports wildcards using `*` and `?`.
  allow_headers_backend: Vec<String>,
  /// Headers that are not forwarded to the backend storage server. Supports wildcards using `*` and `?`.
  deny_headers_backend: Vec<String>,
  /// Headers that are reflected back to the client in tickets. Supports wildcards using `*` and `?`.
  allow_headers_client: Vec<String>,
  /// Headers that are not reflected back to the client in tickets. Supports wildcards using `*` and `?`.
  deny_headers_client: Vec<String>,
  #[serde(skip_serializing)]
  #[schemars(skip)]
  client: HttpClient,
  /// Optional Crypt4GH keys to use when decrypting data.
  #[cfg(feature = "experimental")]
  #[serde(skip_serializing)]
  keys: Option<C4GHKeys>,
  /// Whether to forward the C4GH public key in a context header.
  #[cfg(feature = "experimental")]
  forward_public_key: bool,
  #[serde(skip)]
  pub(crate) is_defaulted: bool,
}

impl Eq for JsonPath {}

impl PartialEq for JsonPath {
  fn eq(&self, other: &Self) -> bool {
    self.resolve_from == other.resolve_from
      && self.content_path == other.content_path
      && self.size_path == other.size_path
      && self.response_path == other.response_path
      && self.allow_headers_backend == other.allow_headers_backend
      && self.deny_headers_backend == other.deny_headers_backend
      && self.allow_headers_client == other.allow_headers_client
      && self.deny_headers_client == other.deny_headers_client
  }
}

impl JsonPath {
  /// Create a new json path storage.
  pub fn new(
    resolve_from: Uri,
    content_path: String,
    size_path: Option<String>,
    response_path: Option<JsonPathOrUrl>,
    allow_headers_backend: Vec<String>,
    allow_headers_client: Vec<String>,
    client: HttpClient,
  ) -> Self {
    Self {
      resolve_from,
      content_path,
      size_path,
      response_path,
      allow_headers_backend,
      deny_headers_backend: vec![],
      allow_headers_client,
      deny_headers_client: vec![],
      client,
      #[cfg(feature = "experimental")]
      keys: None,
      #[cfg(feature = "experimental")]
      forward_public_key: true,
      is_defaulted: false,
    }
  }

  /// Set the headers blocked from being forwarded to the backend server.
  pub fn set_deny_headers_backend(&mut self, deny_headers_backend: Vec<String>) {
    self.deny_headers_backend = deny_headers_backend;
  }

  /// Set the headers blocked from being reflected back to the client.
  pub fn set_deny_headers_client(&mut self, deny_headers_client: Vec<String>) {
    self.deny_headers_client = deny_headers_client;
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

  /// Get the headers forwarded to the backend storage server. A wildcard value of "*" forwards
  /// all headers. Defaults to `["*"]`.
  pub fn allow_headers_backend(&self) -> &[String] {
    &self.allow_headers_backend
  }

  /// Get the headers blocked from being forwarded to the backend storage server. Supports wildcards
  /// using `*` and `?`. Defaults to `[]`.
  pub fn deny_headers_backend(&self) -> &[String] {
    &self.deny_headers_backend
  }

  /// Get the headers reflected back to the client in tickets. A wildcard value of "*" reflects
  /// all headers. Defaults to `["*"]`.
  pub fn allow_headers_client(&self) -> &[String] {
    &self.allow_headers_client
  }

  /// Get the headers blocked from being reflected back to the client in tickets. Supports wildcards
  /// using `*` and `?`. Defaults to `[]`.
  pub fn deny_headers_client(&self) -> &[String] {
    &self.deny_headers_client
  }

  /// Get an owned client by cloning.
  pub fn client_cloned(&mut self) -> Result<ClientWithMiddleware> {
    self.client.as_inner_built().cloned()
  }

  /// Get a mutable reference to the inner client builder.
  pub fn inner_client_mut(&mut self) -> &mut HttpClient {
    &mut self.client
  }

  /// Set the C4GH keys.
  #[cfg(feature = "experimental")]
  pub fn set_keys(&mut self, keys: Option<C4GHKeys>) {
    self.keys = keys;
  }

  /// Get the C4GH keys.
  #[cfg(feature = "experimental")]
  pub fn keys(&self) -> Option<&C4GHKeys> {
    self.keys.as_ref()
  }

  /// Get a mutable reference to the C4GH keys.
  #[cfg(feature = "experimental")]
  pub fn keys_mut(&mut self) -> &mut Option<C4GHKeys> {
    &mut self.keys
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

impl Default for JsonPath {
  fn default() -> Self {
    let mut json_path = Self::new(
      Default::default(),
      Default::default(),
      Default::default(),
      Default::default(),
      vec!["*".to_string()],
      vec!["*".to_string()],
      HttpClient::from(HttpClientConfig::default()),
    );

    #[cfg(feature = "experimental")]
    {
      json_path.set_forward_public_key(true);
    }

    json_path.is_defaulted = true;
    json_path
  }
}
