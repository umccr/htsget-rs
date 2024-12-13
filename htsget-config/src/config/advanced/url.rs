use crate::error::Error;
use crate::error::Error::ParseError;
use crate::error::Result;
use crate::storage;
#[cfg(feature = "experimental")]
use crate::storage::c4gh::C4GHKeys;
use crate::tls::client::TlsClientConfig;
use cfg_if::cfg_if;
use http::Uri;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Url {
  #[serde(with = "http_serde::uri")]
  url: Uri,
  #[serde(with = "http_serde::option::uri", default)]
  response_url: Option<Uri>,
  #[serde(default = "default_forward_headers")]
  forward_headers: bool,
  #[serde(default)]
  header_blacklist: Vec<String>,
  #[serde(skip_serializing, default)]
  tls: TlsClientConfig,
  #[cfg(feature = "experimental")]
  #[serde(skip_serializing, default)]
  keys: Option<C4GHKeys>,
}

impl Url {
  /// Create a new url storage.
  pub fn new(
    url: Uri,
    response_url: Option<Uri>,
    forward_headers: bool,
    header_blacklist: Vec<String>,
    tls: TlsClientConfig,
  ) -> Self {
    Self {
      url,
      response_url,
      forward_headers,
      header_blacklist,
      tls,
      #[cfg(feature = "experimental")]
      keys: None,
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

impl TryFrom<Url> for storage::url::Url {
  type Error = Error;

  fn try_from(storage: Url) -> Result<Self> {
    let mut builder = Client::builder();

    let (certs, identity) = storage.tls.into_inner();

    if let Some(certs) = certs {
      for cert in certs {
        builder = builder.add_root_certificate(cert);
      }
    }
    if let Some(identity) = identity {
      builder = builder.identity(identity);
    }

    let client = builder
      .build()
      .map_err(|err| ParseError(format!("building url storage client: {}", err)))?;

    let url_storage = Self::new(
      storage.url.clone(),
      storage.response_url.unwrap_or(storage.url),
      storage.forward_headers,
      storage.header_blacklist,
      client,
    );

    cfg_if! {
      if #[cfg(feature = "experimental")] {
        Ok(url_storage.set_keys(storage.keys))
      } else {
        Ok(url_storage)
      }
    }
  }
}

fn default_forward_headers() -> bool {
  true
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
