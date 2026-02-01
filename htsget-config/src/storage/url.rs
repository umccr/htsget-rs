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
  /// Whether to forward client headers to the remote URL.
  forward_headers: bool,
  /// Headers to not forward to the remote URL even if `forward_headers` is true.
  header_blacklist: Vec<String>,
  #[serde(skip_serializing)]
  #[schemars(skip)]
  client: HttpClient,
  /// Optional Crypt4GH keys to use when decrypting data.
  #[cfg(feature = "experimental")]
  #[serde(skip_serializing)]
  keys: Option<C4GHKeys>,
  #[serde(skip)]
  pub(crate) is_defaulted: bool,
}

impl Eq for Url {}

impl PartialEq for Url {
  fn eq(&self, other: &Self) -> bool {
    self.url == other.url
      && self.response_url == other.response_url
      && self.forward_headers == other.forward_headers
      && self.header_blacklist == other.header_blacklist
  }
}

impl Url {
  /// Create a new url storage client.
  pub fn new(
    url: Uri,
    response_url: Uri,
    forward_headers: bool,
    header_blacklist: Vec<String>,
    client: HttpClient,
  ) -> Self {
    Self {
      url,
      response_url,
      forward_headers,
      header_blacklist,
      client,
      #[cfg(feature = "experimental")]
      keys: None,
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

  /// Whether to forward headers in the url tickets.
  pub fn forward_headers(&self) -> bool {
    self.forward_headers
  }

  /// Get the headers that should not be forwarded.
  pub fn header_blacklist(&self) -> &[String] {
    &self.header_blacklist
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
