//! JWT authorization configuration and response structures.
//!
//! This module provides configuration structures for JWT token validation and authorization
//! service integration, enabling fine-grained access control over genomic data.

use crate::config::{deserialize_vec_from_str, serialize_array_display};
use crate::error::{Error::ParseError, Result};
use crate::tls::client::TlsClientConfig;
use http::Uri;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod response;

pub use response::{AuthorizationResponse, AuthorizationRule, ReferenceNameRestriction};

/// Configuration for JWT authorization.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct AuthConfig {
  #[serde(with = "http_serde::option::uri", default)]
  jwks_url: Option<Uri>,
  decode_public_key: Option<PathBuf>,
  validate_audience: Option<Vec<String>>,
  validate_issuer: Option<Vec<String>>,
  #[serde(
    serialize_with = "serialize_array_display",
    deserialize_with = "deserialize_vec_from_str",
    default
  )]
  trusted_authorization_urls: Vec<Uri>,
  authorization_path: Option<String>,
  #[serde(skip_serializing, default)]
  tls: TlsClientConfig,
}

impl AuthConfig {
  /// Create a new auth config.
  pub fn new(
    jwks_url: Option<Uri>,
    decode_public_key: Option<PathBuf>,
    validate_audience: Option<Vec<String>>,
    validate_issuer: Option<Vec<String>>,
    trusted_authorization_urls: Vec<Uri>,
    authorization_path: Option<String>,
    tls: TlsClientConfig,
  ) -> Self {
    Self {
      jwks_url,
      decode_public_key,
      validate_audience,
      validate_issuer,
      trusted_authorization_urls,
      authorization_path,
      tls,
    }
  }

  /// Get the JWKS URL.
  pub fn jwks_url(&self) -> Option<&Uri> {
    self.jwks_url.as_ref()
  }

  /// Get the decode public key path.
  pub fn decode_public_key(&self) -> Option<&Path> {
    self.decode_public_key.as_deref()
  }

  /// Get the validate audience list.
  pub fn validate_audience(&self) -> Option<&[String]> {
    self.validate_audience.as_deref()
  }

  /// Get the validate issuer list.
  pub fn validate_issuer(&self) -> Option<&[String]> {
    self.validate_issuer.as_deref()
  }

  /// Get the trusted authorization URLs.
  pub fn trusted_authorization_urls(&self) -> &[Uri] {
    &self.trusted_authorization_urls
  }

  /// Get the authorization path.
  pub fn authorization_path(&self) -> Option<&str> {
    self.authorization_path.as_deref()
  }

  /// Get the TLS config.
  pub fn tls(&self) -> &TlsClientConfig {
    &self.tls
  }

  /// Validate the auth configuration.
  pub fn validate(&self) -> Result<()> {
    match (&self.jwks_url, &self.decode_public_key) {
      (None, None) => {
        return Err(ParseError(
          "Either 'jwks_url' or 'decode_public_key' must be set".to_string(),
        ));
      }
      (Some(_), Some(_)) => {
        return Err(ParseError(
          "Cannot set both 'jwks_url' and 'decode_public_key'".to_string(),
        ));
      }
      _ => {}
    }

    // Validate trusted_authorization_urls contains at least one URL
    if self.trusted_authorization_urls.is_empty() {
      return Err(ParseError(
        "At least one URL must be provided in 'trusted_authorization_urls'".to_string(),
      ));
    }

    Ok(())
  }
}

impl Default for AuthConfig {
  fn default() -> Self {
    Self {
      jwks_url: None,
      decode_public_key: None,
      validate_audience: None,
      validate_issuer: None,
      trusted_authorization_urls: vec!["https://example.com/".parse().expect("valid uri")],
      authorization_path: None,
      tls: TlsClientConfig::default(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::tests::test_serialize_and_deserialize;
  use std::fs::File;
  use tempfile::tempdir;

  #[test]
  fn auth_config_serialization() {
    test_serialize_and_deserialize(
      r#"
            jwks_url = "https://www.example.com"
            validate_audience = ["aud1", "aud2"]
            validate_issuer = ["iss1"]
            trusted_authorization_urls = ["https://www.example.com"]
            authorization_path = "$.auth_url"
            "#,
      (
        Some("https://www.example.com/".to_string()),
        Some(vec!["aud1".to_string(), "aud2".to_string()]),
        Some(vec!["iss1".to_string()]),
        vec!["https://www.example.com".parse().unwrap()],
        Some("$.auth_url".to_string()),
      ),
      |result: AuthConfig| {
        (
          result.jwks_url().map(|s| s.to_string()),
          result.validate_audience().map(|v| v.to_vec()),
          result.validate_issuer().map(|v| v.to_vec()),
          result.trusted_authorization_urls().to_vec(),
          result.authorization_path().map(|s| s.to_string()),
        )
      },
    );
  }

  #[test]
  fn auth_config_with_public_key() {
    let temp_dir = tempdir().unwrap();
    let key_path = temp_dir.path().join("key.pem");
    File::create(&key_path).unwrap();

    let config = AuthConfig::new(
      None,
      Some(key_path.clone()),
      None,
      None,
      vec!["https://www.example.com".parse().unwrap()],
      None,
      Default::default(),
    );

    assert_eq!(config.decode_public_key(), Some(key_path.as_path()));
    assert!(config.validate().is_ok());
  }

  #[test]
  fn auth_config_validation_missing_jwt_method() {
    let config = AuthConfig::new(
      None,
      None,
      None,
      None,
      vec!["https://www.example.com".parse().unwrap()],
      None,
      Default::default(),
    );

    let result = config.validate();
    assert!(result.is_err());
  }

  #[test]
  fn auth_config_validation_both_jwt_methods() {
    let temp_dir = tempdir().unwrap();
    let key_path = temp_dir.path().join("key.pem");
    File::create(&key_path).unwrap();

    let config = AuthConfig::new(
      Some("https://www.example.com".parse().unwrap()),
      Some(key_path),
      None,
      None,
      vec!["https://www.example.com".parse().unwrap()],
      None,
      Default::default(),
    );

    let result = config.validate();
    assert!(result.is_err());
  }

  #[test]
  fn auth_config_validation_empty_trusted_urls() {
    let config = AuthConfig::new(
      Some("https://www.example.com".parse().unwrap()),
      None,
      None,
      None,
      vec![],
      None,
      Default::default(),
    );

    let result = config.validate();
    assert!(result.is_err());
  }

  #[test]
  fn auth_config_validation_success_jwks() {
    let config = AuthConfig::new(
      Some("https://www.example.com/".parse().unwrap()),
      None,
      Some(vec!["aud1".to_string()]),
      Some(vec!["iss1".to_string()]),
      vec!["https://www.example.com".parse().unwrap()],
      Some("$.auth_url".to_string()),
      Default::default(),
    );

    assert!(config.validate().is_ok());
  }

  #[test]
  fn auth_config_validation_success_public_key() {
    let temp_dir = tempdir().unwrap();
    let key_path = temp_dir.path().join("key.pem");
    File::create(&key_path).unwrap();

    let config = AuthConfig::new(
      None,
      Some(key_path),
      Some(vec!["aud1".to_string()]),
      Some(vec!["iss1".to_string()]),
      vec!["https://www.example.com".parse().unwrap()],
      Some("$.auth_url".to_string()),
      Default::default(),
    );

    assert!(config.validate().is_ok());
  }
}
