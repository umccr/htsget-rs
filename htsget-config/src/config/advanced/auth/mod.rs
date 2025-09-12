//! JWT authorization configuration and response structures.
//!
//! This module provides configuration structures for JWT token validation and authorization
//! service integration, enabling fine-grained access control over genomic data.
//!

use crate::config::advanced::HttpClient;
use crate::config::advanced::auth::authorization::{ForwardExtensions, UrlOrStatic};
use crate::config::advanced::auth::jwt::AuthMode;
use crate::error::{Error, Result};
use crate::http::client::HttpClientConfig;
use reqwest_middleware::ClientWithMiddleware;
pub use response::{AuthorizationRestrictions, AuthorizationRule, ReferenceNameRestriction};
use serde::Deserialize;

pub mod authorization;
pub mod jwt;
pub mod response;

/// Configuration for JWT authorization.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, try_from = "AuthConfigBuilder")]
pub struct AuthConfig {
  auth_mode: Option<AuthMode>,
  validate_audience: Option<Vec<String>>,
  validate_issuer: Option<Vec<String>>,
  validate_subject: Option<String>,
  authorization_url: Option<UrlOrStatic>,
  forward_headers: Vec<String>,
  passthrough_auth: bool,
  forward_extensions: Vec<ForwardExtensions>,
  http_client: HttpClient,
  #[cfg(feature = "experimental")]
  suppress_errors: bool,
  #[cfg(feature = "experimental")]
  add_hint: bool,
}

impl AuthConfig {
  /// Whether to suppress errors and return any available regions.
  #[cfg(feature = "experimental")]
  pub fn suppress_errors(&self) -> bool {
    self.suppress_errors
  }

  /// Whether the client gets a hint about which regions are allowed.
  #[cfg(feature = "experimental")]
  pub fn add_hint(&self) -> bool {
    self.add_hint
  }

  /// Get the http client.
  pub fn http_client(&self) -> &ClientWithMiddleware {
    &self.http_client.0
  }

  /// Get the authorization mode.
  pub fn auth_mode(&self) -> Option<&AuthMode> {
    self.auth_mode.as_ref()
  }

  /// Get the authorization mode.
  pub fn auth_mode_mut(&mut self) -> Option<&mut AuthMode> {
    self.auth_mode.as_mut()
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

  /// Get the authorization url.
  pub fn authorization_url(&self) -> Option<&UrlOrStatic> {
    self.authorization_url.as_ref()
  }

  /// Get the headers to forward.
  pub fn forward_headers(&self) -> &[String] {
    self.forward_headers.as_slice()
  }

  /// Get whether to pass through the auth header.
  pub fn passthrough_auth(&self) -> bool {
    self.passthrough_auth
  }

  /// Get the extensions to forward.
  pub fn forward_extensions(&self) -> &[ForwardExtensions] {
    self.forward_extensions.as_slice()
  }
}

/// Builder for `AuthConfig`.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct AuthConfigBuilder {
  #[serde(flatten, skip_serializing)]
  auth_mode: Option<AuthMode>,
  validate_audience: Option<Vec<String>>,
  validate_issuer: Option<Vec<String>>,
  validate_subject: Option<String>,
  authorization_url: Option<UrlOrStatic>,
  forward_headers: Vec<String>,
  passthrough_auth: bool,
  forward_extensions: Vec<ForwardExtensions>,
  #[serde(rename = "http", alias = "tls", skip_serializing)]
  http_client: Option<HttpClient>,
  #[cfg(feature = "experimental")]
  suppress_errors: bool,
  #[cfg(feature = "experimental")]
  add_hint: bool,
}

impl AuthConfigBuilder {
  /// Set the HTTP client.
  pub fn http_client(mut self, http_client: HttpClient) -> Self {
    self.http_client = Some(http_client);
    self
  }

  /// Suppress errors and return any allowed regions if available.
  #[cfg(feature = "experimental")]
  pub fn suppress_errors(mut self, suppress_errors: bool) -> Self {
    self.suppress_errors = suppress_errors;
    self
  }

  /// Add a hint that shows the client which regions are allowed in ticket responses.
  #[cfg(feature = "experimental")]
  pub fn add_hint(mut self, add_hint: bool) -> Self {
    self.add_hint = add_hint;
    self
  }

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

  /// Set the authorization url.
  pub fn authorization_url(mut self, authorization_url: UrlOrStatic) -> Self {
    self.authorization_url = Some(authorization_url);
    self
  }

  /// Set the headers to forward.
  pub fn forward_headers(mut self, forward_headers: Vec<String>) -> Self {
    self.forward_headers = forward_headers;
    self
  }

  /// Set whether to pass through auth.
  pub fn passthrough_auth(mut self, passthrough_auth: bool) -> Self {
    self.passthrough_auth = passthrough_auth;
    self
  }

  /// Set the extensions to forward
  pub fn forward_extensions(mut self, forward_extensions: Vec<ForwardExtensions>) -> Self {
    self.forward_extensions = forward_extensions;
    self
  }

  /// Build the auth config.
  pub fn build(self) -> Result<AuthConfig> {
    Ok(AuthConfig {
      auth_mode: self.auth_mode,
      validate_audience: self.validate_audience,
      validate_issuer: self.validate_issuer,
      validate_subject: self.validate_subject,
      authorization_url: self.authorization_url,
      forward_headers: self.forward_headers,
      passthrough_auth: self.passthrough_auth,
      forward_extensions: self.forward_extensions,
      http_client: self
        .http_client
        .unwrap_or(HttpClient::try_from(HttpClientConfig::default())?),
      #[cfg(feature = "experimental")]
      suppress_errors: self.suppress_errors,
      #[cfg(feature = "experimental")]
      add_hint: self.add_hint,
    })
  }
}

impl Default for AuthConfigBuilder {
  fn default() -> Self {
    Self {
      auth_mode: None,
      validate_audience: None,
      validate_issuer: None,
      validate_subject: None,
      authorization_url: None,
      forward_headers: vec![],
      passthrough_auth: true,
      forward_extensions: vec![],
      http_client: None,
      #[cfg(feature = "experimental")]
      suppress_errors: false,
      #[cfg(feature = "experimental")]
      add_hint: true,
    }
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
  use crate::config::advanced::auth::response::{
    AuthorizationRestrictionsBuilder, AuthorizationRuleBuilder,
  };
  use crate::http::tests::with_test_certificates;
  use http::Uri;
  use serde_json::to_string;
  use std::io::Write;
  use tempfile::NamedTempFile;

  #[test]
  fn auth_config_public_key() {
    with_test_certificates(|path, _, _| {
      let key_path = path.join("key.pem");

      let config: AuthConfig = toml::from_str(&format!(
        r#"
        public_key = "{}"
        "#,
        key_path.to_string_lossy()
      ))
      .unwrap();

      assert!(matches!(
        config.auth_mode().unwrap(),
        AuthMode::PublicKey(_)
      ));
    });
  }

  #[test]
  fn auth_config_no_mode() {
    let config = toml::from_str::<AuthConfig>(
      r#"
      validate_audience = ["aud1", "aud2"]
      validate_issuer = ["iss1"]
      validate_subject = sub
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
      "#,
    );
    assert!(config.is_err());
  }

  #[test]
  fn auth_config_no_authentication() {
    let config: AuthConfig = toml::from_str(
      r#"
      authorization_url = "https://www.example.com"
      "#,
    )
    .unwrap();

    assert_eq!(
      config.authorization_url().unwrap(),
      &UrlOrStatic::Url("https://www.example.com".parse::<Uri>().unwrap())
    );
  }

  #[test]
  fn auth_config_static_auth() {
    let mut temp = NamedTempFile::new().unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(
        AuthorizationRuleBuilder::default()
          .path("path")
          .build()
          .unwrap(),
      )
      .build()
      .unwrap();
    temp
      .write_all(to_string(&restrictions).unwrap().as_bytes())
      .unwrap();

    let config: AuthConfig = toml::from_str(&format!(
      r#"
      authorization_url = "file://{}"
      "#,
      temp.path().to_string_lossy()
    ))
    .unwrap();

    assert_eq!(
      config.authorization_url().unwrap(),
      &UrlOrStatic::Static(restrictions)
    );
  }

  #[test]
  fn auth_config() {
    let config: AuthConfig = toml::from_str(
      r#"
      jwks_url = "https://www.example.com"
      validate_audience = ["aud1", "aud2"]
      validate_issuer = ["iss1"]
      validate_subject = "sub"
      authorization_url = "https://www.example.com"
      "#,
    )
    .unwrap();

    assert_eq!(
      config.auth_mode().unwrap(),
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
      config.authorization_url().unwrap(),
      &UrlOrStatic::Url("https://www.example.com".parse::<Uri>().unwrap())
    );
  }

  #[cfg(feature = "experimental")]
  #[test]
  fn auth_config_experimental() {
    let config: AuthConfig = toml::from_str(
      r#"
      jwks_url = "https://www.example.com"
      validate_audience = ["aud1", "aud2"]
      validate_issuer = ["iss1"]
      authorization_url = "https://www.example.com"
      add_hint = false
      suppress_errors = true
      "#,
    )
    .unwrap();

    assert!(!config.add_hint());
    assert!(config.suppress_errors());
  }

  #[test]
  fn test_authorization_restrictions_builder() {
    let rule = AuthConfigBuilder::default()
      .auth_mode(AuthMode::Jwks("https://www.example.com/".parse().unwrap()))
      .authorization_url(UrlOrStatic::Url(
        "https://www.example.com".parse::<Uri>().unwrap(),
      ))
      .build()
      .unwrap();
    assert_eq!(
      rule.authorization_url.as_ref().unwrap(),
      &UrlOrStatic::Url("https://www.example.com".parse::<Uri>().unwrap())
    );
    assert_eq!(
      rule.clone().auth_mode.unwrap(),
      AuthMode::Jwks("https://www.example.com/".parse().unwrap())
    );
    assert_eq!(rule.validate_audience(), None);
    assert_eq!(rule.validate_issuer(), None);
    assert_eq!(rule.validate_subject(), None);
  }
}
