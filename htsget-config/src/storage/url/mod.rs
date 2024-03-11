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
use crate::storage::url::endpoints::Endpoints;
use crate::tls::TlsClientConfig;

pub mod endpoints;

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
  endpoints: Endpoints,
  response_url: ValidatedUrl,
  forward_headers: bool,
  user_agent: Option<String>,
  #[serde(skip_serializing)]
  tls: TlsClientConfig,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "UrlStorage")]
pub struct UrlStorageClient {
  endpoints: Endpoints,
  response_url: ValidatedUrl,
  forward_headers: bool,
  user_agent: Option<String>,
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
      storage.endpoints,
      storage.response_url,
      storage.forward_headers,
      storage.user_agent,
      client,
    )
  }
}

impl UrlStorageClient {
  /// Create a new url storage client.
  pub fn new(
    endpoints: Endpoints,
    response_url: ValidatedUrl,
    forward_headers: bool,
    user_agent: Option<String>,
    client: Client<HttpsConnector<HttpConnector>>,
  ) -> Self {
    Self {
      endpoints,
      response_url,
      forward_headers,
      user_agent,
      client,
    }
  }

  /// Get the endpoints config.
  pub fn endpoints(&self) -> &Endpoints {
    &self.endpoints
  }

  /// Get the response url to return to the client
  pub fn response_url(&self) -> &InnerUrl {
    &self.response_url.0.inner
  }

  /// Whether to forward headers in the url tickets.
  pub fn forward_headers(&self) -> bool {
    self.forward_headers
  }

  /// Get the user agent.
  pub fn user_agent(&self) -> Option<String> {
    self.user_agent.clone()
  }

  /// Get a cloned copy of the http client.
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

impl From<InnerUrl> for ValidatedUrl {
  fn from(url: InnerUrl) -> Self {
    ValidatedUrl(Url { inner: url })
  }
}

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
    endpoints: Endpoints,
    response_url: InnerUrl,
    forward_headers: bool,
    user_agent: Option<String>,
    tls: TlsClientConfig,
  ) -> Self {
    Self {
      endpoints,
      response_url: ValidatedUrl(Url {
        inner: response_url,
      }),
      forward_headers,
      user_agent,
      tls,
    }
  }

  /// Get the endpoints config.
  pub fn endpoints(&self) -> &Endpoints {
    &self.endpoints
  }

  /// Get the response url which is returned to the client.
  pub fn response_url(&self) -> &InnerUrl {
    &self.response_url.0.inner
  }

  /// Whether headers received in a query request should be
  /// included in the returned data block tickets.
  pub fn forward_headers(&self) -> bool {
    self.forward_headers
  }

  /// Get the user agent.
  pub fn user_agent(&self) -> Option<&str> {
    self.user_agent.as_deref()
  }

  /// Get the tls client config.
  pub fn tls(&self) -> &TlsClientConfig {
    &self.tls
  }
}

impl Default for UrlStorage {
  fn default() -> Self {
    Self {
      endpoints: Default::default(),
      response_url: default_url(),
      forward_headers: true,
      user_agent: None,
      tls: TlsClientConfig::default(),
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::config::tests::test_config_from_file;
  use crate::storage::Storage;
  use crate::tls::tests::with_test_certificates;

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
        response_url = "https://example.com/"
        forward_headers = false
        user_agent = "user-agent"
        tls.key = "{}"
        tls.cert = "{}"
        tls.root_store = "{}"

        [resolvers.storage.endpoints]
        head = "https://example.com/"
        file = "https://example.com/"
        index = "https://example.com/"
        "#,
          key_path.to_string_lossy().escape_default(),
          cert_path.to_string_lossy().escape_default(),
          cert_path.to_string_lossy().escape_default()
        ),
        |config| {
          println!("{:?}", config.resolvers().first().unwrap().storage());
          assert!(matches!(
              config.resolvers().first().unwrap().storage(),
              Storage::Url { url_storage } if *url_storage.endpoints().file() == "https://example.com/"
                && !url_storage.forward_headers() && url_storage.user_agent() == Some("user-agent".to_string())
          ));
        },
      );
    });
  }
}
