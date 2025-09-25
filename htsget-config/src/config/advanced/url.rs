//! The config for remote URL server locations.
//!

use crate::config::advanced::HttpClient;
use crate::error::Error;
use crate::error::Result;
use crate::http::client::HttpClientConfig;
use crate::storage;
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use cfg_if::cfg_if;
use http::Uri;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Options for the remote URL server config.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Url {
  #[schemars(with = "String")]
  #[serde(with = "http_serde::uri")]
  url: Uri,
  #[schemars(with = "Option::<String>")]
  #[serde(with = "http_serde::option::uri")]
  response_url: Option<Uri>,
  forward_headers: bool,
  header_blacklist: Vec<String>,
  #[schemars(skip)]
  #[serde(alias = "tls", skip_serializing)]
  http: HttpClientConfig,
  #[cfg(feature = "experimental")]
  #[serde(skip_serializing)]
  keys: Option<C4GHKeys>,
  #[serde(skip)]
  pub(crate) is_defaulted: bool,
}

impl Url {
  /// Create a new url storage.
  pub fn new(
    url: Uri,
    response_url: Option<Uri>,
    forward_headers: bool,
    header_blacklist: Vec<String>,
    http: HttpClientConfig,
  ) -> Self {
    Self {
      url,
      response_url,
      forward_headers,
      header_blacklist,
      http,
      #[cfg(feature = "experimental")]
      keys: None,
      is_defaulted: false,
    }
  }

  /// Get the url called when resolving the query.
  pub fn url(&self) -> &Uri {
    &self.url
  }

  /// Get the response url which is returned to the client.
  pub fn response_url(&self) -> Option<&Uri> {
    self.response_url.as_ref()
  }

  /// Whether headers received in a query request should be
  /// included in the returned data block tickets.
  pub fn forward_headers(&self) -> bool {
    self.forward_headers
  }

  /// Get the http client config.
  pub fn http(&self) -> &HttpClientConfig {
    &self.http
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

impl TryFrom<Url> for storage::url::Url {
  type Error = Error;

  fn try_from(storage: Url) -> Result<Self> {
    let client = HttpClient::try_from(storage.http)?.0;

    let url_storage = Self::new(
      storage.url.clone(),
      storage.response_url.unwrap_or(storage.url),
      storage.forward_headers,
      storage.header_blacklist,
      client,
    );

    cfg_if! {
      if #[cfg(feature = "experimental")] {
        let mut url_storage = url_storage;
        url_storage.set_keys(storage.keys);
        Ok(url_storage)
      } else {
        Ok(url_storage)
      }
    }
  }
}

impl Default for Url {
  fn default() -> Self {
    let mut url = Self::new(
      Default::default(),
      Default::default(),
      true,
      Default::default(),
      Default::default(),
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
  fn url_backend() {
    test_serialize_and_deserialize(
      r#"
      url = "https://example.com"
      response_url = "https://example.com"
      forward_headers = false
      header_blacklist = ["Host"]
      "#,
      (
        "https://example.com/".to_string(),
        "https://example.com/".to_string(),
        false,
        vec!["Host".to_string()],
      ),
      |result: Url| {
        (
          result.url().to_string(),
          result.response_url().unwrap().to_string(),
          result.forward_headers(),
          result.header_blacklist,
        )
      },
    );
  }
}
