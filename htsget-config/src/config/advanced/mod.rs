//! Configuration options that are advanced in the documentation.
//!

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crate::http::client::HttpClientConfig;
use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use serde::{Deserialize, Serialize};
use std::env::temp_dir;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

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
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, try_from = "HttpClientConfig")]
pub struct HttpClient(ClientWithMiddleware);

impl HttpClient {
  /// Create a new client.
  pub fn new(client: ClientWithMiddleware) -> Self {
    Self(client)
  }

  /// Get the inner client value.
  pub fn into_inner(self) -> ClientWithMiddleware {
    self.0
  }
}

impl TryFrom<HttpClientConfig> for HttpClient {
  type Error = Error;

  fn try_from(config: HttpClientConfig) -> Result<Self> {
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

    let client = builder
      .build()
      .map_err(|err| ParseError(format!("building http client: {err}")))?;

    let client = if use_cache {
      let client_cache = temp_dir().join("htsget_rs_client_cache");
      ClientBuilder::new(client)
        .with(Cache(HttpCache {
          mode: CacheMode::Default,
          manager: CACacheManager::new(client_cache, false),
          options: HttpCacheOptions::default(),
        }))
        .build()
    } else {
      ClientBuilder::new(client).build()
    };

    Ok(Self::new(client))
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
