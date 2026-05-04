//! Configuration for remote URL server storage.
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

/// Configure the server to reach out to a remote URL to fetch data.
#[derive(JsonSchema, Deserialize, Serialize, Debug, Clone)]
#[serde(try_from = "advanced::url::Url", deny_unknown_fields)]
pub struct Url {
  /// The URL to fetch data from.
  #[schemars(with = "String")]
  #[serde(with = "http_serde::uri")]
  url: Uri,
  /// The URL of the response tickets.
  #[schemars(with = "String")]
  #[serde(with = "http_serde::uri")]
  response_url: Uri,
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

impl Eq for Url {}

impl PartialEq for Url {
  fn eq(&self, other: &Self) -> bool {
    self.url == other.url
      && self.response_url == other.response_url
      && self.allow_headers_backend == other.allow_headers_backend
      && self.deny_headers_backend == other.deny_headers_backend
      && self.allow_headers_client == other.allow_headers_client
      && self.deny_headers_client == other.deny_headers_client
  }
}

impl Url {
  /// Create a new url storage client.
  pub fn new(
    url: Uri,
    response_url: Uri,
    allow_headers_backend: Vec<String>,
    deny_headers_backend: Vec<String>,
    allow_headers_client: Vec<String>,
    deny_headers_client: Vec<String>,
    client: HttpClient,
  ) -> Self {
    Self {
      url,
      response_url,
      allow_headers_backend,
      deny_headers_backend,
      allow_headers_client,
      deny_headers_client,
      client,
      #[cfg(feature = "experimental")]
      keys: None,
      #[cfg(feature = "experimental")]
      forward_public_key: true,
      is_defaulted: false,
    }
  }

  /// Get the url called when resolving the query.
  pub fn url(&self) -> &Uri {
    &self.url
  }

  /// Get the response url to return to the client
  pub fn response_url(&self) -> &Uri {
    &self.response_url
  }

  /// Get the headers forwarded to the backend storage server. Supports wildcards using `*` and `?`.
  pub fn allow_headers_backend(&self) -> &[String] {
    &self.allow_headers_backend
  }

  /// Get the headers blocked from being forwarded to the backend storage server. Supports
  /// wildcards using `*` and `?`.
  pub fn deny_headers_backend(&self) -> &[String] {
    &self.deny_headers_backend
  }

  /// Get the headers reflected back to the client in tickets. Supports wildcards using `*` and `?`.
  pub fn allow_headers_client(&self) -> &[String] {
    &self.allow_headers_client
  }

  /// Get the headers blocked from being reflected back to the client in tickets. Supports
  /// wildcards using `*` and `?`.
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

impl Default for Url {
  fn default() -> Self {
    let mut url = Self::new(
      Default::default(),
      Default::default(),
      vec!["*".to_string()],
      vec![],
      vec!["*".to_string()],
      vec![],
      HttpClient::from(HttpClientConfig::default()),
    );

    #[cfg(feature = "experimental")]
    {
      url.set_forward_public_key(true);
    }

    url.is_defaulted = true;
    url
  }
}
