//! Configuration options that are advanced in the documentation.
//!

use crate::config::advanced::callout::CacheStore;
use crate::config::service_info::PackageInfo;
use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crate::http::client::HttpClientConfig;
use http::request;
use http_cache::{CacheKey, MokaCacheBuilder, MokaManager};
use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest::Client;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env::temp_dir;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

pub mod auth;
pub mod callout;
pub mod cors;
pub mod regex_location;
#[cfg(feature = "url")]
pub mod url;

/// The prefix used for context header values.
pub const CONTEXT_HEADER_PREFIX: &str = "Htsget-Context-";

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

/// Build a cache key from request parts and forwarded header names.
pub fn build_cache_key(parts: &request::Parts, header_names: &[String]) -> String {
  // Of the headers which would be forwarded, get the header name and value for
  // the request itself.
  let mut cache_headers: Vec<(String, String)> = header_names
      .iter()
      .filter_map(|name| {
        parts
            .headers
            .get(name.as_str())
            .map(|v| (name.clone(), v.to_str().unwrap_or("").to_string()))
      })
      .collect();
  cache_headers.sort_by(|a, b| a.0.cmp(&b.0));

  // Hashing headers to prevert data leak for on-disk cache.
  let mut hasher = Sha256::new();
  for (name, value) in &cache_headers {
    let hash = format!("{}-{}-{}-{}", parts.method, parts.uri, name, value);
    hasher.update(hash);
  }
  let result = hasher.finalize();
  result.iter().map(|b| format!("{:x}", b)).collect()
}

/// A wrapper around a reqwest client to support creating from config fields.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, from = "HttpClientConfig")]
pub struct HttpClient {
  config: Option<HttpClientConfig>,
  client: Option<ClientWithMiddleware>,
  ttl_ceiling_secs: Option<u64>,
}

impl HttpClient {
  /// Create a new client.
  pub fn new(client: ClientWithMiddleware) -> Self {
    Self {
      config: None,
      client: Some(client),
      ttl_ceiling_secs: None,
    }
  }

  /// Set the client from an incomplete builder.
  pub fn new_with_config(config: HttpClientConfig) -> Self {
    Self {
      config: Some(config),
      client: None,
      ttl_ceiling_secs: None,
    }
  }

  /// Get the client builder by taking out the config value.
  pub fn take_config(&mut self) -> Result<HttpClientConfig> {
    self
      .config
      .take()
      .ok_or_else(|| ParseError("client already built".to_string()))
  }

  /// Set the builder.
  pub fn set_config(&mut self, config: HttpClientConfig) {
    self.config = Some(config);
  }

  /// Get the inner client, building it if necessary.
  pub fn as_inner_built(&mut self) -> Result<&ClientWithMiddleware> {
    self.as_inner_built_with_forwarded_headers(&[])
  }

  /// Build the inner client with the configured CachePolicy and identity based
  /// cache from forwarded header names.
  pub fn as_inner_built_with_forwarded_headers(
    &mut self,
    forwarded_header_names: &[String],
  ) -> Result<&ClientWithMiddleware> {
    if let Some(ref client) = self.client {
      return Ok(client);
    }

    let config = self.take_config()?;
    let mut builder = Client::builder();

    let (certs, identity, use_cache, user_agent, cache_policy) = config.into_inner();
    self.ttl_ceiling_secs = Some(cache_policy.ttl_ceiling_secs());

    if let Some(certs) = certs {
      for cert in certs {
        builder = builder.add_root_certificate(cert);
      }
    }
    if let Some(identity) = identity {
      builder = builder.identity(identity);
    }
    if let Some(user_agent) = user_agent {
      builder = builder.user_agent(user_agent);
    }

    let inner_client = builder
      .build()
      .map_err(|err| ParseError(format!("building http client: {err}")))?;

    let client = if use_cache {
      let header_names: Vec<String> = forwarded_header_names
        .iter()
        .map(|n| n.to_lowercase())
        .collect();

      let cache_key: CacheKey = Arc::new(move |parts: &request::Parts| {
        build_cache_key(parts, &header_names)
      });

      let options = HttpCacheOptions {
        cache_key: Some(cache_key),
        ..Default::default()
      };

      match cache_policy.store() {
        CacheStore::InMemory { capacity } => {
          let moka_cache = MokaCacheBuilder::default()
            .max_capacity(*capacity)
            .build();
          reqwest_middleware::ClientBuilder::new(inner_client)
            .with(Cache(HttpCache {
              mode: CacheMode::Default,
              manager: MokaManager::new(moka_cache),
              options,
            }))
            .build()
        }
        CacheStore::Disk => {
          let client_cache = temp_dir().join("htsget_rs_client_cache");
          reqwest_middleware::ClientBuilder::new(inner_client)
            .with(Cache(HttpCache {
              mode: CacheMode::Default,
              manager: CACacheManager::new(client_cache, false),
              options,
            }))
            .build()
        }
      }
    } else {
      reqwest_middleware::ClientBuilder::new(inner_client).build()
    };

    self.client = Some(client);
    Ok(self.client.as_ref().expect("expected client"))
  }

  /// Set the user-agent information from the package info.
  pub fn set_from_package_info(&mut self, info: &PackageInfo) -> Result<()> {
    let builder = self.take_config()?;
    self.set_config(builder.with_user_agent(info.id.to_string()));

    Ok(())
  }

  /// Get the TTL ceiling in seconds.
  pub fn ttl_ceiling_secs(&self) -> Option<u64> {
    self.ttl_ceiling_secs
  }
}

impl From<HttpClientConfig> for HttpClient {
  fn from(config: HttpClientConfig) -> Self {
    Self::new_with_config(config)
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::advanced::callout::{CachePolicy, CacheStore};
  use crate::http::client::HttpClientConfig;
  use http::request;

  /// Build Parts from method, URI and headers.
  fn build_parts(method: &str, uri: &str, headers: Vec<(&str, &str)>) -> request::Parts {
    let mut builder = http::Request::builder().method(method).uri(uri);
    for (name, value) in headers {
      builder = builder.header(name, value);
    }
    let (parts, _) = builder.body(()).unwrap().into_parts();
    parts
  }

  #[test]
  fn cache_key_per_requester_isolation() {
    let forwarded = vec!["authorization".to_string(), "htsget-context-id".to_string()];

    let parts_alice = build_parts(
      "GET",
      "https://auth.example.com/check",
      vec![
        ("authorization", "Bearer alice-token"),
        ("htsget-context-id", "sample1"),
      ],
    );
    let parts_bob = build_parts(
      "GET",
      "https://auth.example.com/check",
      vec![
        ("authorization", "Bearer bob-token"),
        ("htsget-context-id", "sample1"),
      ],
    );

    // Different identities must produce different cache keys.
    assert_ne!(
      build_cache_key(&parts_alice, &forwarded),
      build_cache_key(&parts_bob, &forwarded)
    );
  }

  #[test]
  fn cache_key_same_identity_same_key() {
    let forwarded = vec!["authorization".to_string()];

    let parts1 = build_parts(
      "GET",
      "https://auth.example.com/check",
      vec![("authorization", "Bearer same-token")],
    );
    let parts2 = build_parts(
      "GET",
      "https://auth.example.com/check",
      vec![("authorization", "Bearer same-token")],
    );

    assert_eq!(
      build_cache_key(&parts1, &forwarded),
      build_cache_key(&parts2, &forwarded)
    );
  }

  #[test]
  fn cache_key_includes_method_and_uri() {
    let forwarded = vec!["authorization".to_string()];

    let parts_get = build_parts(
      "GET",
      "https://auth.example.com/check",
      vec![("authorization", "Bearer token")],
    );
    let parts_post = build_parts(
      "POST",
      "https://auth.example.com/check",
      vec![("authorization", "Bearer token")],
    );
    let parts_different_uri = build_parts(
      "GET",
      "https://auth.example.com/other",
      vec![("authorization", "Bearer token")],
    );

    // Different methods produce different keys.
    assert_ne!(
      build_cache_key(&parts_get, &forwarded),
      build_cache_key(&parts_post, &forwarded)
    );
    // Different URIs produce different keys.
    assert_ne!(
      build_cache_key(&parts_get, &forwarded),
      build_cache_key(&parts_different_uri, &forwarded)
    );
  }

  #[test]
  fn cache_key_hashed_identity_no_raw_values() {
    let forwarded = vec!["authorization".to_string()];

    let parts = build_parts(
      "GET",
      "https://auth.example.com/check",
      vec![("authorization", "Bearer my-secret-token-abc123")],
    );

    let key = build_cache_key(&parts, &forwarded);

    // The cache key must not contain the raw header value.
    assert!(
      !key.contains("my-secret-token-abc123"),
      "cache key should not contain raw header value: {key}"
    );
  }

  #[test]
  fn cache_key_header_order_independent() {
    // Headers are sorted by name before hashing, so order should not matter.
    let forwarded = vec!["x-header-a".to_string(), "x-header-b".to_string()];

    let parts1 = build_parts(
      "GET",
      "https://auth.example.com/check",
      vec![("x-header-a", "value-a"), ("x-header-b", "value-b")],
    );
    let parts2 = build_parts(
      "GET",
      "https://auth.example.com/check",
      vec![("x-header-b", "value-b"), ("x-header-a", "value-a")],
    );

    assert_eq!(
      build_cache_key(&parts1, &forwarded),
      build_cache_key(&parts2, &forwarded)
    );
  }

  #[test]
  fn use_cache_false_passthrough() {
    // When use_cache is false, building should produce a client with no cache middleware.
    let config = HttpClientConfig::new(None, None, false);
    let mut http_client = HttpClient::new_with_config(config);

    let result =
      http_client.as_inner_built_with_forwarded_headers(&["authorization".to_string()]);
    assert!(result.is_ok());
    // We cannot inspect the middleware stack directly, but the client was built successfully.
  }

  #[test]
  fn build_client_with_moka_manager() {
    let policy = CachePolicy::new(3600, CacheStore::InMemory { capacity: 50 });
    let config = HttpClientConfig::new_with_cache(None, None, true, policy);
    let mut http_client = HttpClient::new_with_config(config);

    let result =
      http_client.as_inner_built_with_forwarded_headers(&["authorization".to_string()]);
    assert!(result.is_ok());
  }

  #[test]
  fn build_client_with_disk_manager() {
    let policy = CachePolicy::new(7200, CacheStore::Disk);
    let config = HttpClientConfig::new_with_cache(None, None, true, policy);
    let mut http_client = HttpClient::new_with_config(config);

    let result =
      http_client.as_inner_built_with_forwarded_headers(&["authorization".to_string()]);
    assert!(result.is_ok());
  }

  #[test]
  fn build_client_default_policy() {
    let config = HttpClientConfig::new(None, None, true);
    let mut http_client = HttpClient::new_with_config(config);

    let result = http_client.as_inner_built();
    assert!(result.is_ok());
  }
}
