//! JWT authorization configuration and response structures.
//!
//! This module provides configuration structures for JWT token validation and authorization
//! service integration, enabling fine-grained access control over genomic data.
//!

use crate::config::advanced::{Bytes, HttpClient};
use crate::config::{deserialize_vec_from_str, serialize_array_display};
use crate::error::Error::{BuilderError, ParseError};
use crate::error::{Error, Result};
use http::Uri;
pub use response::{AuthorizationRestrictions, AuthorizationRule, ReferenceNameRestriction};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod response;

/// The method for authorization, either using a JWKS url or a public key.
#[derive(Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(
  deny_unknown_fields,
  try_from = "AuthModeSerde",
  into = "AuthModeSerde"
)]
pub enum AuthMode {
  Jwks(Uri),
  PublicKey(Vec<u8>),
}

/// Used to deserialize into the `AuthMode` struct.
#[derive(Deserialize, Debug, Clone, Eq, PartialEq, Default)]
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
        "Either 'jwks_url' or 'public_key' must be set".to_string(),
      )),
      (Some(_), Some(_)) => Err(ParseError(
        "Cannot set both 'jwks_url' and 'public_key'".to_string(),
      )),
      (Some(jwks_url), None) => Ok(AuthMode::Jwks(jwks_url)),
      (None, Some(public_key)) => Ok(AuthMode::PublicKey(
        Bytes::try_from(public_key)?.into_inner(),
      )),
    }
  }
}

/// Configuration for JWT authorization.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, try_from = "AuthConfigBuilder")]
pub struct AuthConfig {
  auth_mode: AuthMode,
  validate_audience: Option<Vec<String>>,
  validate_issuer: Option<Vec<String>>,
  validate_subject: Option<String>,
  trusted_authorization_urls: Vec<Uri>,
  authorization_path: Option<String>,
  http_client: HttpClient,
}

impl AuthConfig {
  /// Get the authorization mode.
  pub fn auth_mode(&self) -> &AuthMode {
    &self.auth_mode
  }

  /// Get the authorization mode.
  pub fn auth_mode_mut(&mut self) -> &mut AuthMode {
    &mut self.auth_mode
  }

  /// Get the validate audience list.
  pub fn validate_audience(&self) -> Option<&[String]> {
    self.validate_audience.as_deref()
  }

  /// Get the validate issuer list.
  pub fn validate_issuer(&self) -> Option<&[String]> {
    self.validate_issuer.as_deref()
  }

  /// Get the validate issuer list.
  pub fn validate_subject(&self) -> Option<&str> {
    self.validate_subject.as_deref()
  }

  /// Get the trusted authorization URLs.
  pub fn trusted_authorization_urls(&self) -> &[Uri] {
    &self.trusted_authorization_urls
  }

  /// Get the authorization path.
  pub fn authorization_path(&self) -> Option<&str> {
    self.authorization_path.as_deref()
  }

  /// Get the http client.
  pub fn http_client(&self) -> &reqwest::Client {
    &self.http_client.0
  }
}

/// Builder for `AuthConfig`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct AuthConfigBuilder {
  #[serde(flatten, skip_serializing)]
  auth_mode: Option<AuthMode>,
  validate_audience: Option<Vec<String>>,
  validate_issuer: Option<Vec<String>>,
  validate_subject: Option<String>,
  #[serde(
    serialize_with = "serialize_array_display",
    deserialize_with = "deserialize_vec_from_str",
    default
  )]
  trusted_authorization_urls: Vec<Uri>,
  authorization_path: Option<String>,
  #[serde(rename = "tls", skip_serializing, default)]
  http_client: Option<HttpClient>,
}

impl AuthConfigBuilder {
  /// Set the auth mode.
  pub fn auth_mode(mut self, auth_mode: AuthMode) -> Self {
    self.auth_mode = Some(auth_mode);
    self
  }

  /// Set audiences to validate.
  pub fn validate_audience(mut self, validate_audience: Vec<String>) -> Self {
    self.validate_audience = Some(validate_audience);
    self
  }

  /// Set the issuers to validate.
  pub fn validate_issuer(mut self, validate_issuer: Vec<String>) -> Self {
    self.validate_issuer = Some(validate_issuer);
    self
  }

  /// Set the subject to validate.
  pub fn validate_subject(mut self, validate_subject: String) -> Self {
    self.validate_subject = Some(validate_subject);
    self
  }

  /// Add an authorization url.
  pub fn trusted_authorization_url(mut self, trusted_authorization_url: Uri) -> Self {
    self
      .trusted_authorization_urls
      .push(trusted_authorization_url);
    self
  }

  /// Add multiple trusted authorization urls.
  pub fn trusted_authorization_urls(mut self, trusted_authorization_urls: Vec<Uri>) -> Self {
    self
      .trusted_authorization_urls
      .extend(trusted_authorization_urls);
    self
  }

  /// Set the authorization JSON path.
  pub fn authorization_path(mut self, authorization_path: String) -> Self {
    self.authorization_path = Some(authorization_path);
    self
  }

  /// Set the HTTP client.
  pub fn http_client(mut self, http_client: HttpClient) -> Self {
    self.http_client = Some(http_client);
    self
  }

  /// Build the auth config.
  pub fn build(self) -> Result<AuthConfig> {
    let Some(auth_mode) = self.auth_mode else {
      return Err(BuilderError("missing auth mode".to_string()));
    };
    if self.trusted_authorization_urls.is_empty() {
      return Err(BuilderError(
        "at least one trusted authorization url must be set".to_string(),
      ));
    }
    if self.authorization_path.is_none() && self.trusted_authorization_urls.len() > 1 {
      return Err(BuilderError(
        "only one trusted authorization url should be set when not using authorization paths"
          .to_string(),
      ));
    }

    Ok(AuthConfig {
      auth_mode,
      validate_audience: self.validate_audience,
      validate_issuer: self.validate_issuer,
      validate_subject: self.validate_subject,
      trusted_authorization_urls: self.trusted_authorization_urls,
      authorization_path: self.authorization_path,
      http_client: HttpClient::default(),
    })
  }
}

impl TryFrom<AuthConfigBuilder> for AuthConfig {
  type Error = Error;

  fn try_from(builder: AuthConfigBuilder) -> Result<Self> {
    builder.build()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::tls::tests::with_test_certificates;

  #[test]
  fn auth_config() {
    let config: AuthConfig = toml::from_str(
      r#"
      jwks_url = "https://www.example.com"
      validate_audience = ["aud1", "aud2"]
      validate_issuer = ["iss1"]
      validate_subject = "sub"
      trusted_authorization_urls = ["https://www.example.com"]
      authorization_path = "$.auth_url"
      "#,
    )
    .unwrap();

    assert_eq!(
      config.auth_mode(),
      &AuthMode::Jwks("https://www.example.com/".parse().unwrap())
    );
    assert_eq!(
      config.validate_audience().unwrap().to_vec(),
      vec!["aud1".to_string(), "aud2".to_string()]
    );
    assert_eq!(
      config.validate_issuer().unwrap().to_vec(),
      vec!["iss1".to_string()]
    );
    assert_eq!(
      config.trusted_authorization_urls().to_vec(),
      vec!["https://www.example.com".parse::<Uri>().unwrap()]
    );
    assert_eq!(config.authorization_path().unwrap(), "$.auth_url");
  }

  #[test]
  fn auth_config_public_key() {
    with_test_certificates(|path, _, _| {
      let key_path = path.join("key.pem");

      let config: AuthConfig = toml::from_str(&format!(
        r#"
            public_key = "{}"
            trusted_authorization_urls = ["https://www.example.com"]
            "#,
        key_path.to_string_lossy()
      ))
      .unwrap();

      assert!(matches!(config.auth_mode(), AuthMode::PublicKey(_)));
      assert_eq!(
        vec!["https://www.example.com".parse::<Uri>().unwrap()],
        config.trusted_authorization_urls().to_vec()
      );
    });
  }

  #[test]
  fn auth_config_no_mode() {
    let config = toml::from_str::<AuthConfig>(
      r#"
      validate_audience = ["aud1", "aud2"]
      validate_issuer = ["iss1"]
      validate_subject = sub
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
      validate_subject = sub
      trusted_authorization_urls = ["https://www.example.com"]
      authorization_path = "$.auth_url"
      "#,
    );
    assert!(config.is_err());
  }

  #[test]
  fn test_authorization_restrictions_builder() {
    let rule = AuthConfigBuilder::default()
      .auth_mode(AuthMode::Jwks("https://www.example.com/".parse().unwrap()))
      .trusted_authorization_url("https://www.example.com".parse().unwrap())
      .build()
      .unwrap();
    assert!(rule.authorization_path.is_none());
    assert_eq!(
      rule.trusted_authorization_urls,
      vec!["https://www.example.com".parse::<Uri>().unwrap()]
    );
    assert_eq!(
      rule.auth_mode,
      AuthMode::Jwks("https://www.example.com/".parse().unwrap())
    );
    assert_eq!(rule.validate_audience(), None);
    assert_eq!(rule.validate_issuer(), None);
    assert_eq!(rule.validate_subject(), None);

    let rule = AuthConfigBuilder::default()
      .trusted_authorization_url("https://www.example.com".parse().unwrap())
      .build();
    assert!(rule.is_err());

    let rule = AuthConfigBuilder::default()
      .auth_mode(AuthMode::Jwks("https://www.example.com/".parse().unwrap()))
      .build();
    assert!(rule.is_err());

    let rule = AuthConfigBuilder::default()
      .auth_mode(AuthMode::Jwks("https://www.example.com/".parse().unwrap()))
      .trusted_authorization_url("https://www.example.com".parse().unwrap())
      .trusted_authorization_url("https://www.example.com".parse().unwrap())
      .build();
    assert!(rule.is_err());
  }
}
