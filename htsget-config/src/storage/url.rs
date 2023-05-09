use crate::storage::local::default_authority;
use crate::types::Scheme;
use http::uri::Authority;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct UrlStorage {
  request_scheme: Scheme,
  response_scheme: Scheme,
  #[serde(with = "http_serde::authority")]
  authority: Authority,
  forward_headers: bool,
}

impl UrlStorage {
  /// Create a new url storage.
  pub fn new(
    request_scheme: Scheme,
    response_scheme: Scheme,
    authority: Authority,
    forward_headers: bool,
  ) -> Self {
    Self {
      request_scheme,
      response_scheme,
      authority,
      forward_headers,
    }
  }

  /// Get the request scheme used in the ticket server.
  pub fn request_scheme(&self) -> Scheme {
    self.request_scheme
  }

  /// Get the response scheme used for data blocks.
  pub fn response_scheme(&self) -> Scheme {
    self.response_scheme
  }

  /// Get the authority called when resolving the query.
  pub fn authority(&self) -> &Authority {
    &self.authority
  }

  /// Whether headers received in a query request should be
  /// included in the returned data block tickets.
  pub fn forward_headers(&self) -> bool {
    self.forward_headers
  }
}

impl Default for UrlStorage {
  fn default() -> Self {
    Self {
      request_scheme: Scheme::Https,
      response_scheme: Scheme::Https,
      authority: default_authority(),
      forward_headers: true,
    }
  }
}
