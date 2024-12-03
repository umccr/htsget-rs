#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use crate::tls::client::TlsClientConfig;
use http::Uri;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UrlStorage {
  #[serde(with = "http_serde::uri")]
  uri: Uri,
  #[serde(with = "http_serde::uri")]
  response_uri: Uri,
  forward_headers: bool,
  header_blacklist: Vec<String>,
  #[serde(skip_serializing)]
  tls: TlsClientConfig,
  #[serde(skip_serializing)]
  #[cfg(feature = "experimental")]
  keys: Option<C4GHKeys>,
}

impl UrlStorage {
  /// Create a new url storage.
  pub fn new(
    uri: Uri,
    response_uri: Uri,
    forward_headers: bool,
    header_blacklist: Vec<String>,
    tls: TlsClientConfig,
  ) -> Self {
    Self {
      uri,
      response_uri,
      forward_headers,
      header_blacklist,
      tls,
      #[cfg(feature = "experimental")]
      keys: None,
    }
  }

  /// Get the uri called when resolving the query.
  pub fn uri(&self) -> &Uri {
    &self.uri
  }

  /// Get the response uri which is returned to the client.
  pub fn response_uri(&self) -> &Uri {
    &self.response_uri
  }

  /// Whether headers received in a query request should be
  /// included in the returned data block tickets.
  pub fn forward_headers(&self) -> bool {
    self.forward_headers
  }

  /// Get the tls client config.
  pub fn tls(&self) -> &TlsClientConfig {
    &self.tls
  }

  #[cfg(feature = "experimental")]
  /// Set the C4GH keys.
  pub fn set_keys(mut self, keys: Option<C4GHKeys>) -> Self {
    self.keys = keys;
    self
  }

  #[cfg(feature = "experimental")]
  /// Get the C4GH keys.
  pub fn keys(&self) -> Option<&C4GHKeys> {
    self.keys.as_ref()
  }
}
