use std::str::FromStr;

use http::Uri as InnerUrl;
use hyper::client::HttpConnector;
use hyper::Client;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use serde::{Deserialize, Serialize};
use serde_with::with_prefix;

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crate::storage::local::default_authority;
use crate::tls::TlsClientConfig;
use crate::types::Scheme;

fn default_url() -> ValidatedUrl {
  ValidatedUrl(Url {
    inner: InnerUrl::from_str(&format!("https://{}", default_authority()))
      .expect("expected valid url"),
  })
}

with_prefix!(client_auth_prefix "client_");

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct UrlStorage {
  url: ValidatedUrl,
  response_scheme: Scheme,
  forward_headers: bool,
  #[serde(skip_serializing)]
  tls: TlsClientConfig,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "UrlStorage")]
pub struct UrlStorageClient {
  url: ValidatedUrl,
  response_scheme: Scheme,
  forward_headers: bool,
  client: Client<HttpsConnector<HttpConnector>>,
}

impl From<UrlStorage> for UrlStorageClient {
  fn from(storage: UrlStorage) -> Self {
    let client = Client::builder().build(
      HttpsConnectorBuilder::new()
        .with_tls_config(storage.tls.into_inner())
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build(),
    );

    Self::new(
      storage.url,
      storage.response_scheme,
      storage.forward_headers,
      client,
    )
  }
}

impl UrlStorageClient {
  /// Create a new url storage client.
  pub fn new(
    url: ValidatedUrl,
    response_scheme: Scheme,
    forward_headers: bool,
    client: Client<HttpsConnector<HttpConnector>>,
  ) -> Self {
    Self {
      url,
      response_scheme,
      forward_headers,
      client,
    }
  }

  /// Get the url called when resolving the query.
  pub fn url(&self) -> &InnerUrl {
    &self.url.0.inner
  }

  /// Get the response scheme used for data blocks.
  pub fn response_scheme(&self) -> Scheme {
    self.response_scheme
  }

  /// Whether to forward headers in the url tickets.
  pub fn forward_headers(&self) -> bool {
    self.forward_headers
  }

  pub fn client_cloned(&self) -> Client<HttpsConnector<HttpConnector>> {
    self.client.clone()
  }
}

/// A wrapper around `http::Uri` type which implements serialize and deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(transparent)]
pub(crate) struct Url {
  #[serde(with = "http_serde::uri")]
  pub(crate) inner: InnerUrl,
}

/// A new type struct on top of `http::Uri` which only allows http or https schemes when deserializing.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "Url")]
pub struct ValidatedUrl(pub(crate) Url);

impl ValidatedUrl {
  /// Get the inner url.
  pub fn into_inner(self) -> InnerUrl {
    self.0.inner
  }
}

impl TryFrom<Url> for ValidatedUrl {
  type Error = Error;

  fn try_from(url: Url) -> Result<Self> {
    match url.inner.scheme() {
      Some(scheme) if scheme == "http" || scheme == "https" => Ok(Self(url)),
      _ => Err(ParseError("url scheme must be http or https".to_string())),
    }
  }
}

impl UrlStorage {
  /// Create a new url storage.
  pub fn new(
    url: InnerUrl,
    response_scheme: Scheme,
    forward_headers: bool,
    tls: TlsClientConfig,
  ) -> Self {
    Self {
      url: ValidatedUrl(Url { inner: url }),
      response_scheme,
      forward_headers,
      tls,
    }
  }

  /// Get the response scheme used for data blocks.
  pub fn response_scheme(&self) -> Scheme {
    self.response_scheme
  }

  /// Get the url called when resolving the query.
  pub fn url(&self) -> &InnerUrl {
    &self.url.0.inner
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
}

impl Default for UrlStorage {
  fn default() -> Self {
    Self {
      url: default_url(),
      response_scheme: Scheme::Https,
      forward_headers: true,
      tls: TlsClientConfig::default(),
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::config::tests::test_config_from_file;
  use crate::storage::Storage;
  use crate::tls::tests::with_test_certificates;
  use crate::types::Scheme;

  #[test]
  fn config_storage_url_file() {
    with_test_certificates(|path, _, _| {
      let key_path = path.join("key.pem");
      let cert_path = path.join("cert.pem");

      test_config_from_file(
        &format!(
          r#"
        [[resolvers]]
        regex = "regex"

        [resolvers.storage]
        url = "https://example.com/"
        response_scheme = "Http"
        forward_headers = false
        tls.key = "{}"
        tls.cert = "{}"
        tls.root_store = "{}"
        "#,
          key_path.to_string_lossy().escape_default(),
          cert_path.to_string_lossy().escape_default(),
          cert_path.to_string_lossy().escape_default()
        ),
        |config| {
          println!("{:?}", config.resolvers().first().unwrap().storage());
          assert!(matches!(
              config.resolvers().first().unwrap().storage(),
              Storage::Url { url_storage } if *url_storage.url() == "https://example.com/"
                && url_storage.response_scheme() == Scheme::Http
                && !url_storage.forward_headers()
          ));
        },
      );
    });
  }
}
