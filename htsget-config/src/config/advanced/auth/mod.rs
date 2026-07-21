//! JWT authorization configuration and response structures.
//!
//! Provides JWT validation and authorization config for accessing
//! data.
//!

use crate::config::advanced::auth::authorization::{
  AuthorizationSource, AuthorizationSourceBuilder,
};
use crate::config::advanced::auth::jwt::{JwtKey, JwtKeyBuilder};
use crate::config::service_info::PackageInfo;
use crate::error::Error::ParseError;
use crate::error::{Error, Result};
pub use response::{AuthorizationRestrictions, AuthorizationRule, ReferenceNameRestriction};
use serde::Deserialize;

pub mod authorization;
pub mod jwt;
pub mod response;

/// Auth configuration using JWT validation with optional authorization.
#[derive(Deserialize, Debug, Clone)]
#[serde(try_from = "AuthConfigBuilder")]
pub struct AuthConfig {
  jwt: Option<JwtKey>,
  validate_audience: Option<Vec<String>>,
  validate_issuer: Option<Vec<String>>,
  validate_subject: Option<String>,
  authorization: Option<AuthorizationSource>,
  #[cfg(feature = "experimental")]
  suppress_errors: bool,
  #[cfg(feature = "experimental")]
  add_hint: bool,
}

impl AuthConfig {
  /// JWT key source.
  pub fn jwt(&self) -> Option<&JwtKey> {
    self.jwt.as_ref()
  }

  /// Mutable JWT key source.
  pub fn jwt_mut(&mut self) -> Option<&mut JwtKey> {
    self.jwt.as_mut()
  }

  /// Audiences to validate.
  pub fn validate_audience(&self) -> Option<&[String]> {
    self.validate_audience.as_deref()
  }

  /// Issuers to validate.
  pub fn validate_issuer(&self) -> Option<&[String]> {
    self.validate_issuer.as_deref()
  }

  /// Subject to validate.
  pub fn validate_subject(&self) -> Option<&str> {
    self.validate_subject.as_deref()
  }

  /// Authorization source.
  pub fn authorization(&self) -> Option<&AuthorizationSource> {
    self.authorization.as_ref()
  }

  /// Mutable authorization source.
  pub fn authorization_mut(&mut self) -> Option<&mut AuthorizationSource> {
    self.authorization.as_mut()
  }

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

  /// Set the user-agent on every internal HTTP client.
  pub fn set_from_package_info(&mut self, info: &PackageInfo) -> Result<()> {
    if let Some(callout) = self.jwt.as_mut().and_then(JwtKey::jwks_mut) {
      callout.http_mut().set_from_package_info(info)?;
    }
    if let Some(callout) = self
      .authorization
      .as_mut()
      .and_then(AuthorizationSource::callout_mut)
    {
      callout.http_mut().set_from_package_info(info)?;
    }
    Ok(())
  }
}

/// Builder for `AuthConfig`.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields, default)]
pub struct AuthConfigBuilder {
  jwt: Option<JwtKeyBuilder>,
  #[serde(skip)]
  jwt_raw: Option<JwtKey>,
  validate_audience: Option<Vec<String>>,
  validate_issuer: Option<Vec<String>>,
  validate_subject: Option<String>,
  authorization: Option<AuthorizationSourceBuilder>,
  #[cfg(feature = "experimental")]
  suppress_errors: bool,
  #[cfg(feature = "experimental")]
  add_hint: bool,
}

impl AuthConfigBuilder {
  /// Set the JWT key from a builder.
  pub fn jwt(mut self, jwt: JwtKeyBuilder) -> Self {
    self.jwt = Some(jwt);
    self
  }

  /// Set a JWT key directly.
  pub fn jwt_raw(mut self, jwt: JwtKey) -> Self {
    self.jwt_raw = Some(jwt);
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

  /// Set the authorization source.
  pub fn authorization(mut self, authorization: AuthorizationSourceBuilder) -> Self {
    self.authorization = Some(authorization);
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

  /// Build the auth config.
  pub fn build(self) -> Result<AuthConfig> {
    let jwt = match (self.jwt, self.jwt_raw) {
      (None, None) => None,
      (Some(builder), None) => Some(builder.build()?),
      (None, Some(key)) => Some(key),
      (Some(_), Some(_)) => {
        return Err(ParseError(
          "specify only one of `jwt` or `jwt_raw`".to_string(),
        ));
      }
    };

    let authorization = self.authorization.map(|b| b.build()).transpose()?;

    Ok(AuthConfig {
      jwt,
      validate_audience: self.validate_audience,
      validate_issuer: self.validate_issuer,
      validate_subject: self.validate_subject,
      authorization,
      #[cfg(feature = "experimental")]
      suppress_errors: self.suppress_errors,
      #[cfg(feature = "experimental")]
      add_hint: self.add_hint,
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
  use crate::config::advanced::auth::response::{
    AuthorizationRestrictionsBuilder, AuthorizationRuleBuilder,
  };
  use crate::config::location::{Location, PrefixOrId, SimpleLocation};
  use crate::http::tests::with_test_certificates;
  use crate::storage::Backend;
  use serde_json::to_string;
  use std::io::Write;
  use tempfile::NamedTempFile;

  #[test]
  fn auth_config_jwks_url() {
    let config: AuthConfig =
      toml::from_str(r#"jwt = { kind = "jwks", url = "https://example.com/jwks" }"#).unwrap();
    let callout = config.jwt().unwrap().jwks().unwrap();
    assert_eq!(callout.url().to_string(), "https://example.com/jwks");
  }

  #[test]
  fn auth_config_jwks_full() {
    let config: AuthConfig = toml::from_str(
      r#"
      [jwt]
      kind = "jwks"
      url  = "https://example.com/jwks"

      [jwt.forward]
      headers.allow = ["Authorization"]
      "#,
    )
    .unwrap();
    let callout = config.jwt().unwrap().jwks().unwrap();
    assert_eq!(callout.url().to_string(), "https://example.com/jwks");
    assert_eq!(
      callout.forward().headers().allow(),
      &["Authorization".to_string()]
    );
  }

  #[test]
  fn auth_config_public_key() {
    with_test_certificates(|path, _, _| {
      let key_path = path.join("key.pem");
      let config: AuthConfig = toml::from_str(&format!(
        r#"
        [jwt]
        kind = "public_key"
        path = '{}'
        "#,
        key_path.to_string_lossy()
      ))
      .unwrap();
      assert!(config.jwt().unwrap().public_key().is_some());
    });
  }

  #[test]
  fn auth_config_rejects_duplicate() {
    let result = AuthConfigBuilder::default()
      .jwt(JwtKeyBuilder::PublicKey {
        path: "key.pem".into(),
      })
      .jwt_raw(JwtKey::PublicKey(vec![1, 2, 3]))
      .build();
    assert!(result.is_err());
  }

  #[test]
  fn auth_config_only_authorization() {
    let config: AuthConfig = toml::from_str(
      r#"
      [authorization]
      kind = "callout"
      url  = "https://example.com/auth"
      "#,
    )
    .unwrap();
    assert!(config.jwt().is_none());
    assert_eq!(
      config
        .authorization()
        .unwrap()
        .callout()
        .unwrap()
        .url()
        .to_string(),
      "https://example.com/auth"
    );
  }

  #[test]
  fn auth_config_authorization_static() {
    let mut temp = NamedTempFile::new().unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(
        AuthorizationRuleBuilder::default()
          .location(Location::Simple(Box::new(SimpleLocation::new(
            Backend::default(),
            String::default(),
            Some(PrefixOrId::Id("path".to_string())),
          ))))
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
      [authorization]
      kind = "static"
      path = '{}'
      "#,
      temp.path().to_string_lossy()
    ))
    .unwrap();

    assert_eq!(
      config.authorization().unwrap().static_restrictions(),
      Some(&restrictions)
    );
  }

  #[test]
  fn auth_config_full() {
    let config: AuthConfig = toml::from_str(
      r#"
      jwt = { kind = "jwks", url = "https://www.example.com/jwks" }
      validate_audience = ["aud1", "aud2"]
      validate_issuer = ["iss1"]
      validate_subject = "sub"

      [authorization]
      kind = "callout"
      url  = "https://www.example.com/auth"

      [authorization.forward]
      headers.allow = ["Authorization", "X-Custom"]

      [authorization.forward.context]
      endpoint_type = true
      id = true
      extensions = [{ json_path = "$.extension" }]
      "#,
    )
    .unwrap();

    let jwks = config.jwt().unwrap().jwks().unwrap();
    assert_eq!(jwks.url().to_string(), "https://www.example.com/jwks");
    assert_eq!(config.validate_audience().unwrap(), &["aud1", "aud2"]);
    assert_eq!(config.validate_issuer().unwrap(), &["iss1"]);
    assert_eq!(config.validate_subject(), Some("sub"));

    let authz = config.authorization().unwrap().callout().unwrap();
    assert_eq!(authz.url().to_string(), "https://www.example.com/auth");
    assert_eq!(
      authz.forward().headers().allow(),
      &["Authorization".to_string(), "X-Custom".to_string()]
    );
    assert!(authz.forward().context().endpoint_type());
    assert!(authz.forward().context().id());
    assert_eq!(authz.forward().context().extensions().len(), 1);
    assert_eq!(
      authz.forward().context().extensions()[0].json_path(),
      "$.extension"
    );
    assert_eq!(
      authz.forward().context().extensions()[0].name(),
      "Extension"
    );
  }

  #[cfg(feature = "experimental")]
  #[test]
  fn auth_config_experimental() {
    let config: AuthConfig = toml::from_str(
      r#"
      jwt = { kind = "jwks", url = "https://www.example.com" }
      add_hint = false
      suppress_errors = true
      "#,
    )
    .unwrap();

    assert!(!config.add_hint());
    assert!(config.suppress_errors());
  }
}
