//! Configuration for remote URL server storage.
//!

use crate::config::advanced;
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use http::Uri;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Remote URL server storage struct.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(try_from = "advanced::url::Url", deny_unknown_fields)]
pub struct Url {
  #[serde(with = "http_serde::uri")]
  url: Uri,
  #[serde(with = "http_serde::uri")]
  response_url: Uri,
  forward_headers: bool,
  header_blacklist: Vec<String>,
  #[serde(skip_serializing)]
  client: Client,
  #[cfg(feature = "experimental")]
  #[serde(skip_serializing)]
  keys: Option<C4GHKeys>,
}

impl Url {
  /// Create a new url storage client.
  pub fn new(
    url: Uri,
    response_url: Uri,
    forward_headers: bool,
    header_blacklist: Vec<String>,
    client: Client,
  ) -> Self {
    Self {
      url,
      response_url,
      forward_headers,
      header_blacklist,
      client,
      #[cfg(feature = "experimental")]
      keys: None,
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
  pub fn client_cloned(&self) -> Client {
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
