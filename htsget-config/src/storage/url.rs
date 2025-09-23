//! Configuration for remote URL server storage.
//!

use crate::config::advanced;
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use http::Uri;
use reqwest_middleware::ClientWithMiddleware;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Remote URL server storage struct.
#[derive(JsonSchema, Deserialize, Serialize, Debug, Clone)]
#[serde(try_from = "advanced::url::Url", deny_unknown_fields)]
pub struct Url {
  #[schemars(with = "String")]
  #[serde(with = "http_serde::uri")]
  url: Uri,
  #[schemars(with = "String")]
  #[serde(with = "http_serde::uri")]
  response_url: Uri,
  forward_headers: bool,
  header_blacklist: Vec<String>,
  #[serde(skip_serializing)]
  #[schemars(skip)]
  client: ClientWithMiddleware,
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
    client: ClientWithMiddleware,
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
  pub fn client_cloned(&self) -> ClientWithMiddleware {
    self.client.clone()
  }

  #[cfg(feature = "experimental")]
  /// Set the C4GH keys.
  pub fn set_keys(&mut self, keys: Option<C4GHKeys>) {
    self.keys = keys;
  }

  #[cfg(feature = "experimental")]
  /// Get the C4GH keys.
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
      Default::default(),
    );
    url.is_defaulted = true;
    url
  }
}
