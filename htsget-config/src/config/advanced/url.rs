//! The config for the URL backend.
//!

use crate::config::advanced::HttpClient;
use crate::config::advanced::callout::{Forward, Parse, Reflect};
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

/// Options for the URL backend.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Url {
  #[schemars(with = "String")]
  #[serde(with = "http_serde::uri")]
  url: Uri,
  parse: Parse,
  forward: Forward,
  reflect: Reflect,
  #[schemars(skip)]
  #[serde(alias = "tls", skip_serializing)]
  http: HttpClientConfig,
  #[cfg(feature = "experimental")]
  #[serde(skip_serializing)]
  keys: Option<C4GHKeys>,
  #[cfg(feature = "experimental")]
  forward_public_key: bool,
  #[serde(skip)]
  pub(crate) is_defaulted: bool,
}

impl Url {
  /// Create a new URL backend.
  pub fn new(
    url: Uri,
    parse: Parse,
    forward: Forward,
    reflect: Reflect,
    http: HttpClientConfig,
  ) -> Self {
    Self {
      url,
      parse,
      forward,
      reflect,
      http,
      #[cfg(feature = "experimental")]
      keys: None,
      #[cfg(feature = "experimental")]
      forward_public_key: true,
      is_defaulted: false,
    }
  }

  /// The URL to fetch from.
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

  /// Get the HTTP client config.
  pub fn http(&self) -> &HttpClientConfig {
    &self.http
  }

  /// Set the C4GH keys.
  #[cfg(feature = "experimental")]
  pub fn set_keys(mut self, keys: Option<C4GHKeys>) -> Self {
    self.keys = keys;
    self
  }

  /// Get the C4GH keys.
  #[cfg(feature = "experimental")]
  pub fn keys(&self) -> Option<&C4GHKeys> {
    self.keys.as_ref()
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

impl TryFrom<Url> for storage::url::Url {
  type Error = Error;

  fn try_from(url: Url) -> Result<Self> {
    let client = HttpClient::from(url.http);

    let storage = Self::new(url.url, url.parse, url.forward, url.reflect, client);

    cfg_if! {
      if #[cfg(feature = "experimental")] {
        let mut storage = storage;
        storage.set_keys(url.keys);
        storage.set_forward_public_key(url.forward_public_key);
        Ok(storage)
      } else {
        Ok(storage)
      }
    }
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::advanced::callout::{HeaderRules, TicketSource};
  use crate::config::tests::test_serialize_and_deserialize;

  #[test]
  fn url_backend_bytes_default() {
    let url: Url = toml::from_str(
      r#"
      url = "https://example.com"
      "#,
    )
    .unwrap();
    assert_eq!(url.url().to_string(), "https://example.com/");
    assert!(matches!(url.parse(), Parse::Bytes { ticket_url: None }));
  }

  #[test]
  fn url_backend_bytes_ticket() {
    let url: Url = toml::from_str(
      r#"
      url = "https://example.com"

      [parse]
      kind = "bytes"
      ticket_url = "https://tickets.example.com"
      "#,
    )
    .unwrap();
    match url.parse() {
      Parse::Bytes {
        ticket_url: Some(uri),
      } => assert_eq!(uri.to_string(), "https://tickets.example.com/"),
      _ => panic!("expected Bytes with ticket_url"),
    }
  }

  #[test]
  fn url_backend_json_path() {
    let url: Url = toml::from_str(
      r#"
      url = "https://example.com"

      [parse]
      kind = "json_path"
      content_path = "$.url"
      size_path = "$.size"
      ticket_path = "$.ticket"
      "#,
    )
    .unwrap();
    match url.parse() {
      Parse::JsonPath {
        content_path,
        size_path,
        ticket: Some(TicketSource::JsonPath { path }),
      } => {
        assert_eq!(content_path, "$.url");
        assert_eq!(size_path.as_deref(), Some("$.size"));
        assert_eq!(path, "$.ticket");
      }
      _ => panic!("expected JsonPath"),
    }
  }

  #[test]
  fn url_backend_forward_reflect() {
    let url: Url = toml::from_str(
      r#"
      url = "https://example.com"

      [forward]
      headers.allow = ["Authorization"]

      [reflect]
      headers.allow = ["X-Etag"]
      "#,
    )
    .unwrap();
    assert_eq!(
      url.forward().headers().allow(),
      &["Authorization".to_string()]
    );
    assert_eq!(url.reflect().headers().allow(), &["X-Etag".to_string()]);
  }

  #[test]
  fn url_round_trip() {
    test_serialize_and_deserialize(
      r#"
      url = "https://example.com"

      [parse]
      kind = "json_path"
      content_path = "$.url"

      [forward]
      headers.allow = ["Authorization"]

      [reflect]
      headers.allow = ["X-Etag"]
      "#,
      (
        "https://example.com/".to_string(),
        "$.url".to_string(),
        HeaderRules::new(vec!["Authorization".to_string()], vec![]),
        HeaderRules::new(vec!["X-Etag".to_string()], vec![]),
      ),
      |result: Url| {
        let content_path = match result.parse() {
          Parse::JsonPath { content_path, .. } => content_path.clone(),
          _ => panic!(),
        };
        (
          result.url().to_string(),
          content_path,
          result.forward().headers().clone(),
          result.reflect().headers().clone(),
        )
      },
    );
  }
}
