//! JWT authorization configuration and response structures.
//!
//! This module provides configuration structures for JWT token validation and authorization
//! service integration, enabling fine-grained access control over genomic data.
//!

use crate::config::{deserialize_vec_from_str, serialize_array_display};
use crate::tls::client::TlsClientConfig;
use http::Uri;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod response;

use crate::error::Error::ParseError;
use crate::error::{Error, Result};
pub use response::{AuthorizationResponse, AuthorizationRule, ReferenceNameRestriction};

/// The method for authorization, either using a JWKS url or a public key.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(
  deny_unknown_fields,
  try_from = "AuthModeSerde",
  into = "AuthModeSerde"
)]
pub enum AuthMode {
  Jwks(Uri),
  PublicKey(PathBuf),
}

/// Used to deserialize into the `AuthMode` struct.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Default)]
#[serde(deny_unknown_fields, default)]
struct AuthModeSerde {
  #[serde(with = "http_serde::option::uri")]
  jwks_url: Option<Uri>,
  public_key: Option<PathBuf>,
}

impl TryFrom<AuthModeSerde> for AuthMode {
  type Error = Error;

  fn try_from(mode: AuthModeSerde) -> Result<Self> {
    match (mode.jwks_url, mode.public_key) {
      (None, None) => Err(ParseError(
        "Either 'jwks_url' or 'decode_public_key' must be set".to_string(),
      )),
      (Some(_), Some(_)) => Err(ParseError(
        "Cannot set both 'jwks_url' and 'decode_public_key'".to_string(),
      )),
      (Some(jwks_url), None) => Ok(AuthMode::Jwks(jwks_url)),
      (None, Some(public_key)) => Ok(AuthMode::PublicKey(public_key)),
    }
  }
}

impl From<AuthMode> for AuthModeSerde {
  fn from(mode: AuthMode) -> Self {
    match mode {
      AuthMode::Jwks(jwks_url) => Self {
        jwks_url: Some(jwks_url),
        public_key: None,
      },
      AuthMode::PublicKey(public_key) => Self {
        public_key: Some(public_key),
        jwks_url: None,
      },
    }
  }
}

/// Configuration for JWT authorization.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
  #[serde(flatten)]
  auth_mode: AuthMode,
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
    auth_mode: AuthMode,
    validate_audience: Option<Vec<String>>,
    validate_issuer: Option<Vec<String>>,
    trusted_authorization_urls: Vec<Uri>,
    authorization_path: Option<String>,
    tls: TlsClientConfig,
  ) -> Self {
    Self {
      auth_mode,
      validate_audience,
      validate_issuer,
      trusted_authorization_urls,
      authorization_path,
      tls,
    }
  }

  /// Get the authorization mode.
  pub fn auth_mode(&self) -> &AuthMode {
    &self.auth_mode
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
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::tests::test_serialize_and_deserialize;

  #[test]
  fn auth_config() {
    test_serialize_and_deserialize(
      r#"
            jwks_url = "https://www.example.com"
            validate_audience = ["aud1", "aud2"]
            validate_issuer = ["iss1"]
            trusted_authorization_urls = ["https://www.example.com"]
            authorization_path = "$.auth_url"
            "#,
      (
        AuthMode::Jwks("https://www.example.com/".parse().unwrap()),
        Some(vec!["aud1".to_string(), "aud2".to_string()]),
        Some(vec!["iss1".to_string()]),
        vec!["https://www.example.com".parse().unwrap()],
        Some("$.auth_url".to_string()),
      ),
      |result: AuthConfig| {
        (
          result.auth_mode().clone(),
          result.validate_audience().map(|v| v.to_vec()),
          result.validate_issuer().map(|v| v.to_vec()),
          result.trusted_authorization_urls().to_vec(),
          result.authorization_path().map(|s| s.to_string()),
        )
      },
    );
  }

  #[test]
  fn auth_config_public_key() {
    test_serialize_and_deserialize(
      r#"
            public_key = "public_key"
            trusted_authorization_urls = ["https://www.example.com"]
            "#,
      (
        AuthMode::PublicKey("public_key".parse().unwrap()),
        vec!["https://www.example.com".parse().unwrap()],
      ),
      |result: AuthConfig| {
        (
          result.auth_mode().clone(),
          result.trusted_authorization_urls().to_vec(),
        )
      },
    );
  }

  #[test]
  fn auth_config_no_mode() {
    let config = toml::from_str::<AuthConfig>(
      r#"
      validate_audience = ["aud1", "aud2"]
      validate_issuer = ["iss1"]
      trusted_authorization_urls = ["https://www.example.com"]
      authorization_path = "$.auth_url"
      "#,
    );
    assert!(config.is_err());
  }

  #[test]
  fn auth_config_both_modes() {
    let config = toml::from_str::<AuthConfig>(
      r#"
      jwks_url = "https://www.example.com"
      public_key = "public_key"
      validate_audience = ["aud1", "aud2"]
      validate_issuer = ["iss1"]
      trusted_authorization_urls = ["https://www.example.com"]
      authorization_path = "$.auth_url"
      "#,
    );
    assert!(config.is_err());
  }
}
