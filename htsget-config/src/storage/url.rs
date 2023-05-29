use std::str::FromStr;

use serde::{Deserialize, Serialize};
use url::Url as InnerUrl;

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crate::storage::local::default_authority;
use crate::types::Scheme;

pub fn default_url() -> Url {
  Url(InnerUrl::from_str(&format!("https://{}", default_authority())).expect("expected valid url"))
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct UrlStorage {
  url: Url,
  response_scheme: Scheme,
  forward_headers: bool,
}

/// A new type struct on top of `url::Url` which only allows http or https schemes when deserializing.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "InnerUrl")]
pub struct Url(InnerUrl);

impl Url {
  /// Get the inner url.
  pub fn into_inner(self) -> InnerUrl {
    self.0
  }
}

impl TryFrom<InnerUrl> for Url {
  type Error = Error;

  fn try_from(url: InnerUrl) -> Result<Self> {
    if url.scheme() == "http" || url.scheme() == "https" {
      Ok(Self(url))
    } else {
      Err(ParseError("url scheme must be http or https".to_string()))
    }
  }
}

impl UrlStorage {
  /// Create a new url storage.
  pub fn new(url: InnerUrl, response_scheme: Scheme, forward_headers: bool) -> Self {
    Self {
      url: Url(url),
      response_scheme,
      forward_headers,
    }
  }

  /// Get the response scheme used for data blocks.
  pub fn response_scheme(&self) -> Scheme {
    self.response_scheme
  }

  /// Get the url called when resolving the query.
  pub fn url(&self) -> &InnerUrl {
    &self.url.0
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
      url: default_url(),
      response_scheme: Scheme::Https,
      forward_headers: true,
    }
  }
}
