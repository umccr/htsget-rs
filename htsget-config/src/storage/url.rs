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
  endpoint_head: ValidatedUrl,
  endpoint_index: ValidatedUrl,
  endpoint_file: ValidatedUrl,
  #[cfg(feature = "crypt4gh")]
  endpoint_crypt4gh_header: Option<ValidatedUrl>,
  response_scheme: Scheme,
  forward_headers: bool,
  #[serde(skip_serializing)]
  tls: TlsClientConfig,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "UrlStorage")]
pub struct UrlStorageClient {
  endpoint_head: ValidatedUrl,
  endpoint_index: ValidatedUrl,
  endpoint_file: ValidatedUrl,
  #[cfg(feature = "crypt4gh")]
  endpoint_crypt4gh_header: Option<ValidatedUrl>,
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
      storage.endpoint_head,
      storage.endpoint_index,
      storage.endpoint_file,
      storage.response_scheme,
      storage.forward_headers,
      client,
      #[cfg(feature = "crypt4gh")]
      storage.endpoint_crypt4gh_header,
    )
  }
}

impl UrlStorageClient {
  /// Create a new url storage client.
  pub fn new(
    endpoint_head: ValidatedUrl,
    endpoint_index: ValidatedUrl,
    endpoint_header: ValidatedUrl,
    response_scheme: Scheme,
    forward_headers: bool,
    client: Client<HttpsConnector<HttpConnector>>,
    #[cfg(feature = "crypt4gh")] endpoint_crypt4gh_header: Option<ValidatedUrl>,
  ) -> Self {
    Self {
      endpoint_head,
      endpoint_index,
      endpoint_file: endpoint_header,
      #[cfg(feature = "crypt4gh")]
      endpoint_crypt4gh_header,
      response_scheme,
      forward_headers,
      client,
    }
  }

  /// Get the url for the index called when resolving the query.
  pub fn endpoint_index(&self) -> &InnerUrl {
    &self.endpoint_index.0.inner
  }

  /// Get the url for head called when resolving the query.
  pub fn endpoint_head(&self) -> &InnerUrl {
    &self.endpoint_head.0.inner
  }

  /// Get the url for underlying file called when resolving the query.
  pub fn endpoint_file(&self) -> &InnerUrl {
    &self.endpoint_file.0.inner
  }

  /// Get the response scheme used for data blocks.
  pub fn response_scheme(&self) -> Scheme {
    self.response_scheme
  }

  /// Whether to forward headers in the url tickets.
  pub fn forward_headers(&self) -> bool {
    self.forward_headers
  }

  /// Get the crypt4gh header url.
  #[cfg(feature = "crypt4gh")]
  pub fn endpoint_crypt4gh_header(&self) -> Option<&InnerUrl> {
    self
      .endpoint_crypt4gh_header
      .as_ref()
      .map(|url| &url.0.inner)
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
    endpoint_head: InnerUrl,
    endpoint_header: InnerUrl,
    endpoint_index: InnerUrl,
    response_scheme: Scheme,
    forward_headers: bool,
    tls: TlsClientConfig,
    #[cfg(feature = "crypt4gh")] endpoint_crypt4gh_header: Option<InnerUrl>,
  ) -> Self {
    Self {
      endpoint_head: ValidatedUrl(Url {
        inner: endpoint_head,
      }),
      endpoint_index: ValidatedUrl(Url {
        inner: endpoint_index,
      }),
      endpoint_file: ValidatedUrl(Url {
        inner: endpoint_header,
      }),
      response_scheme,
      forward_headers,
      tls,
      #[cfg(feature = "crypt4gh")]
      endpoint_crypt4gh_header: endpoint_crypt4gh_header
        .map(|url| ValidatedUrl(Url { inner: url })),
    }
  }

  /// Get the response scheme used for data blocks.
  pub fn response_scheme(&self) -> Scheme {
    self.response_scheme
  }

  /// Get the endpoint file called when resolving the query.
  pub fn endpoint_file(&self) -> &InnerUrl {
    &self.endpoint_file.0.inner
  }

  /// Get the endpoint for the index called when resolving the query.
  pub fn endpoint_index(&self) -> &InnerUrl {
    &self.endpoint_index.0.inner
  }

  /// Get the endpoint for the head called when resolving the query.
  pub fn endpoint_head(&self) -> &InnerUrl {
    &self.endpoint_head.0.inner
  }

  /// Get the endpoint crypt4gh header called when resolving the query.
  #[cfg(feature = "crypt4gh")]
  pub fn endpoint_crypt4gh_header(&self) -> Option<&InnerUrl> {
    self
      .endpoint_crypt4gh_header
      .as_ref()
      .map(|header| &header.0.inner)
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
      endpoint_file: default_url(),
      endpoint_index: default_url(),
      endpoint_head: default_url(),
      response_scheme: Scheme::Https,
      forward_headers: true,
      tls: TlsClientConfig::default(),
      #[cfg(feature = "crypt4gh")]
      endpoint_crypt4gh_header: None,
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
        endpoint_head = "https://example.com/"
        endpoint_header = "https://example.com/"
        endpoint_index = "https://example.com/"
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
              Storage::Url { url_storage } if *url_storage.endpoint_file() == "https://example.com/"
                && url_storage.response_scheme() == Scheme::Http
                && !url_storage.forward_headers()
          ));
        },
      );
    });
  }
}
