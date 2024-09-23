use std::str::FromStr;

use http::Uri as InnerUrl;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_with::with_prefix;

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crate::storage::local::default_authority;
use crate::tls::client::TlsClientConfig;

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
  response_url: ValidatedUrl,
  forward_headers: bool,
  header_blacklist: Vec<String>,
  #[serde(skip_serializing)]
  tls: TlsClientConfig,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(try_from = "UrlStorage")]
pub struct UrlStorageClient {
  url: ValidatedUrl,
  response_url: ValidatedUrl,
  forward_headers: bool,
  header_blacklist: Vec<String>,
  client: Client,
}

impl TryFrom<UrlStorage> for UrlStorageClient {
  type Error = Error;

  fn try_from(storage: UrlStorage) -> Result<Self> {
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

    Ok(Self::new(
      storage.url,
      storage.response_url,
      storage.forward_headers,
      storage.header_blacklist,
      client,
    ))
  }
}

impl UrlStorageClient {
  /// Create a new url storage client.
  pub fn new(
    url: ValidatedUrl,
    response_url: ValidatedUrl,
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
    }
  }

  /// Get the url called when resolving the query.
  pub fn url(&self) -> &InnerUrl {
    &self.url.0.inner
  }

  /// Get the response url to return to the client
  pub fn response_url(&self) -> &InnerUrl {
    &self.response_url.0.inner
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
    response_url: InnerUrl,
    forward_headers: bool,
    header_blacklist: Vec<String>,
    tls: TlsClientConfig,
  ) -> Self {
    Self {
      url: ValidatedUrl(Url { inner: url }),
      response_url: ValidatedUrl(Url {
        inner: response_url,
      }),
      forward_headers,
      header_blacklist,
      tls,
    }
  }

  /// Get the url called when resolving the query.
  pub fn url(&self) -> &InnerUrl {
    &self.url.0.inner
  }

  /// Get the response url which is returned to the client.
  pub fn response_url(&self) -> &InnerUrl {
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
      response_url: default_url(),
      forward_headers: true,
      header_blacklist: vec![],
      tls: TlsClientConfig::default(),
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::config::tests::test_config_from_file;
  use crate::storage::url::{UrlStorage, UrlStorageClient};
  use crate::storage::Storage;
  use crate::tls::client::tests::client_config_from_path;

  use crate::tls::tests::with_test_certificates;

  use super::*;

  #[tokio::test]
  async fn test_building_client() {
    with_test_certificates(|path, _, _| {
      let client_config = client_config_from_path(path);
      let url_storage = UrlStorageClient::try_from(UrlStorage::new(
        "https://example.com".parse::<InnerUrl>().unwrap(),
        "https://example.com".parse::<InnerUrl>().unwrap(),
        true,
        vec![],
        client_config,
      ));

      assert!(url_storage.is_ok());
    });
  }

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
        type = "Url"
        url = "https://example.com/"
        response_url = "https://example.com/"
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
              Storage::Url(url_storage) if *url_storage.url() == "https://example.com/"
                && !url_storage.forward_headers()
          ));
        },
      );
    });
  }
}
