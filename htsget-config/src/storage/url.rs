//! Configuration for a remote URL storage server, supporting raw bytes and
//! JSONPath manifest modes via [`Parse`].
//!

use crate::config::advanced;
use crate::config::advanced::HttpClient;
use crate::config::advanced::callout::{Forward, Parse, Reflect};
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

/// URL-backed storage. Replaces the previous `url` and `json_path` backends.
#[derive(JsonSchema, Deserialize, Serialize, Debug, Clone)]
#[serde(try_from = "advanced::url::Url", deny_unknown_fields)]
pub struct Url {
  /// The URL to fetch data from.
  #[schemars(with = "String")]
  #[serde(with = "http_serde::uri")]
  url: Uri,
  /// How to interpret the response from `url`.
  parse: Parse,
  /// What request data is forwarded to the backend.
  forward: Forward,
  /// What response data is reflected back to the client in tickets.
  reflect: Reflect,
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

impl Eq for Url {}

impl PartialEq for Url {
  fn eq(&self, other: &Self) -> bool {
    self.url == other.url
      && self.parse == other.parse
      && self.forward == other.forward
      && self.reflect == other.reflect
  }
}

impl Url {
  /// Create a new URL storage backend.
  pub fn new(
    url: Uri,
    parse: Parse,
    forward: Forward,
    reflect: Reflect,
    client: HttpClient,
  ) -> Self {
    Self {
      url,
      parse,
      forward,
      reflect,
      client,
      #[cfg(feature = "experimental")]
      keys: None,
      #[cfg(feature = "experimental")]
      forward_public_key: true,
      is_defaulted: false,
    }
  }

  /// The URL to fetch data from.
  pub fn url(&self) -> &Uri {
    &self.url
  }

  /// How to interpret the response.
  pub fn parse(&self) -> &Parse {
    &self.parse
  }

  /// What to forward to the backend.
  pub fn forward(&self) -> &Forward {
    &self.forward
  }

  /// What to reflect back to the client.
  pub fn reflect(&self) -> &Reflect {
    &self.reflect
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

impl Default for Url {
  fn default() -> Self {
    let mut url = Self::new(
      Default::default(),
      Default::default(),
      Default::default(),
      Default::default(),
      HttpClient::from(HttpClientConfig::default()),
    );

    url.is_defaulted = true;
    url
  }
}
