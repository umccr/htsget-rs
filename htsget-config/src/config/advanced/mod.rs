//! Configuration options that are advanced in the documentation.
//!

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crate::http::client::HttpClientConfig;
use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest::{Client, ClientBuilder};
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use std::env::temp_dir;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

pub mod allow_guard;
pub mod auth;
pub mod cors;
pub mod regex_location;
#[cfg(feature = "url")]
pub mod url;

/// Determines which tracing formatting style to use.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub enum FormattingStyle {
  #[default]
  Full,
  Compact,
  Pretty,
  Json,
}

/// A wrapper around a reqwest client to support creating from config fields.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, from = "HttpClientConfig")]
pub struct HttpClient {
  use_cache: bool,
  builder: Option<Arc<ClientBuilder>>,
  client: Option<ClientWithMiddleware>,
}

impl HttpClient {
  /// Create a new client.
  pub fn new(client: ClientWithMiddleware) -> Self {
    Self {
      use_cache: false,
      builder: None,
      client: Some(client),
    }
  }

  /// Set the client from an incomplete builder.
  pub fn new_with_builder(builder: ClientBuilder, use_cache: bool) -> Self {
    Self {
      use_cache,
      builder: Some(Arc::new(builder)),
      client: None,
    }
  }

  /// Get the client builder.
  pub fn builder(&mut self) -> Result<ClientBuilder> {
    let err = || ParseError("client already built".to_string());
    Arc::try_unwrap(self.builder.take().ok_or_else(err)?).map_err(|_| err())
  }

  /// Set the builder.
  pub fn set_builder(&mut self, builder: ClientBuilder) {
    self.builder = Some(Arc::new(builder));
  }

  /// Get the inner client, building it if necessary.
  pub fn as_inner_built(&mut self) -> Result<&ClientWithMiddleware> {
    if let Some(ref client) = self.client {
      return Ok(client);
    }

    let inner_client = self
      .builder()?
      .build()
      .map_err(|err| ParseError(format!("building http client: {err}")))?;
    let client = if self.use_cache {
      let client_cache = temp_dir().join("htsget_rs_client_cache");
      reqwest_middleware::ClientBuilder::new(inner_client)
        .with(Cache(HttpCache {
          mode: CacheMode::Default,
          manager: CACacheManager::new(client_cache, false),
          options: HttpCacheOptions::default(),
        }))
        .build()
    } else {
      reqwest_middleware::ClientBuilder::new(inner_client).build()
    };

    self.client = Some(client);
    Ok(self.client.as_ref().expect("expected client"))
  }
}

impl From<HttpClientConfig> for HttpClient {
  fn from(config: HttpClientConfig) -> Self {
    let mut builder = Client::builder();

    let (certs, identity, use_cache) = config.into_inner();

    if let Some(certs) = certs {
      for cert in certs {
        builder = builder.add_root_certificate(cert);
      }
    }
    if let Some(identity) = identity {
      builder = builder.identity(identity);
    }

    let client = builder.user_agent(format!(
      "{}/{}",
      env!("CARGO_PKG_NAME"),
      env!("CARGO_PKG_VERSION")
    ));
    Self::new_with_builder(client, use_cache)
  }
}

/// A wrapper around byte data to support reading files in config.
pub struct Bytes(Vec<u8>);

impl Bytes {
  /// Create a new data wrapper.
  pub fn new(data: Vec<u8>) -> Self {
    Self(data)
  }

  /// Get the bytes.
  pub fn into_inner(self) -> Vec<u8> {
    self.0
  }
}

impl TryFrom<PathBuf> for Bytes {
  type Error = Error;

  fn try_from(path: PathBuf) -> Result<Self> {
    let mut bytes = vec![];
    File::open(path)?.read_to_end(&mut bytes)?;
    Ok(Self(bytes))
  }
}
