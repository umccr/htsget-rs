//! Authorization response data structures for JWT authorization.
//!
//! This module provides data structures for parsing and validating authorization
//! responses from external authorization services.
//!

use crate::error::Error::BuilderError;
use crate::error::Result;
use crate::types::{Format, Interval};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Authorization restrictions from an external authorization service.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationRestrictions {
  /// The version of the schema.
  #[validate(range(min = 1))]
  version: u32,
  /// The authorization rules.
  #[serde(rename = "htsgetAuth")]
  #[validate(length(min = 1))]
  htsget_auth: Vec<AuthorizationRule>,
}

/// Individual authorization rule defining access permissions.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationRule {
  /// The path that the authorization applies to. This should not contain the `/reads` or `/variants` component of the path, and it can be a regex.
  #[validate(length(min = 1))]
  path: String,
  /// The reference name restrictions to apply to this path.
  #[serde(rename = "referenceNames", skip_serializing_if = "Option::is_none")]
  reference_names: Option<Vec<ReferenceNameRestriction>>,
}

/// Restriction on genomic reference names and coordinate ranges.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceNameRestriction {
  /// The reference name to allow.
  #[validate(length(min = 1))]
  name: String,
  /// The format to allow. Allows all formats if unspecified.
  #[serde(skip_serializing_if = "Option::is_none")]
  format: Option<Format>,
  /// The interval to allow. Allows all intervals if unspecified.
  #[serde(flatten)]
  interval: Interval,
}

impl AuthorizationRestrictions {
  /// Get the version of the authorization response format.
  pub fn version(&self) -> u32 {
    self.version
  }

  /// Get the authorization rules.
  pub fn htsget_auth(&self) -> &[AuthorizationRule] {
    &self.htsget_auth
  }
}

impl AuthorizationRule {
  /// Get the file path pattern that this rule allows access to.
  pub fn path(&self) -> &str {
    &self.path
  }

  /// Get the optional restrictions on reference names and genomic coordinates.
  pub fn reference_names(&self) -> Option<&[ReferenceNameRestriction]> {
    self.reference_names.as_deref()
  }
}

impl ReferenceNameRestriction {
  /// Get the name of the reference sequence.
  pub fn name(&self) -> &str {
    &self.name
  }

  /// Get the optional format restriction.
  pub fn format(&self) -> Option<Format> {
    self.format
  }

  /// Get the interval to allow.
  pub fn interval(&self) -> &Interval {
    &self.interval
  }
}

/// Builder for `AuthorizationRestrictions`.
#[derive(Debug, Clone, Default)]
pub struct AuthorizationRestrictionsBuilder {
  version: Option<u32>,
  htsget_auth: Vec<AuthorizationRule>,
}

impl AuthorizationRestrictionsBuilder {
  /// Set the version of the authorization response format.
  pub fn version(mut self, version: u32) -> Self {
    self.version = Some(version);
    self
  }

  /// Add an authorization rule.
  pub fn rule(mut self, rule: AuthorizationRule) -> Self {
    self.htsget_auth.push(rule);
    self
  }

  /// Add multiple authorization rules.
  pub fn rules(mut self, rules: Vec<AuthorizationRule>) -> Self {
    self.htsget_auth.extend(rules);
    self
  }

  /// Build the `AuthorizationRestrictions`.
  pub fn build(self) -> Result<AuthorizationRestrictions> {
    if self.htsget_auth.is_empty() {
      return Err(BuilderError("empty htsget authorization".to_string()));
    }
    if self.version.is_some_and(|version| version < 1) {
      return Err(BuilderError("version must be greater than 1".to_string()));
    }

    Ok(AuthorizationRestrictions {
      version: self.version.unwrap_or(1),
      htsget_auth: self.htsget_auth,
    })
  }
}

/// Builder for `AuthorizationRule`.
#[derive(Debug, Clone, Default)]
pub struct AuthorizationRuleBuilder {
  path: Option<String>,
  reference_names: Vec<ReferenceNameRestriction>,
}

impl AuthorizationRuleBuilder {
  /// Set the path pattern for this rule.
  pub fn path<S: Into<String>>(mut self, path: S) -> Self {
    self.path = Some(path.into());
    self
  }

  /// Add a reference name restriction.
  pub fn reference_name(mut self, restriction: ReferenceNameRestriction) -> Self {
    self.reference_names.push(restriction);
    self
  }

  /// Add multiple reference name restrictions.
  pub fn reference_names(mut self, restrictions: Vec<ReferenceNameRestriction>) -> Self {
    self.reference_names.extend(restrictions);
    self
  }

  /// Build the `AuthorizationRule`.
  pub fn build(self) -> Result<AuthorizationRule> {
    Ok(AuthorizationRule {
      path: self
        .path
        .ok_or_else(|| BuilderError("path not set".to_string()))?,
      reference_names: if self.reference_names.is_empty() {
        None
      } else {
        Some(self.reference_names)
      },
    })
  }
}

/// Builder for `ReferenceNameRestriction`.
#[derive(Debug, Clone, Default)]
pub struct ReferenceNameRestrictionBuilder {
  name: Option<String>,
  format: Option<Format>,
  start: Option<u32>,
  end: Option<u32>,
}

impl ReferenceNameRestrictionBuilder {
  /// Set the reference name.
  pub fn name<S: Into<String>>(mut self, name: S) -> Self {
    self.name = Some(name.into());
    self
  }

  /// Set the format restriction.
  pub fn format(mut self, format: Format) -> Self {
    self.format = Some(format);
    self
  }

  /// Set the start position.
  pub fn start(mut self, start: u32) -> Self {
    self.start = Some(start);
    self
  }

  /// Set the end position.
  pub fn end(mut self, end: u32) -> Self {
    self.end = Some(end);
    self
  }

  /// Build the `ReferenceNameRestriction`.
  pub fn build(self) -> Result<ReferenceNameRestriction> {
    if let (Some(ref start), Some(ref end)) = (self.start, self.end) {
      if start >= end {
        return Err(BuilderError("start must be less than end".to_string()));
      }
    }

    Ok(ReferenceNameRestriction {
      name: self
        .name
        .ok_or_else(|| BuilderError("name not set".to_string()))?,
      format: self.format,
      interval: Interval::new(self.start, self.end),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json;

  #[test]
  fn test_authorization_response_deserialization() {
    let json_value = serde_json::json!({
      "version": 1,
      "htsgetAuth": [{
        "path": "/path/to/file",
        "referenceNames": [{
          "name": "chr1",
          "start": 1000,
          "end": 2000,
          "format": "BAM"
        }]
      }]
    });
    let response: AuthorizationRestrictions = serde_json::from_value(json_value).unwrap();

    assert_eq!(response.version(), 1);
    assert_eq!(response.htsget_auth().len(), 1);
    assert_eq!(response.htsget_auth()[0].path(), "/path/to/file");

    let restrictions = response.htsget_auth()[0].reference_names().unwrap();
    assert_eq!(restrictions.len(), 1);
    assert_eq!(restrictions[0].name(), "chr1");
    assert_eq!(restrictions[0].interval().start(), Some(1000));
    assert_eq!(restrictions[0].interval().end(), Some(2000));
    assert_eq!(restrictions[0].format(), Some(Format::Bam));

    let no_restrictions_value = serde_json::json!({
      "version": 1,
      "htsgetAuth": [{
        "path": "/path/to/file"
      }]
    });
    let no_restrictions_response: AuthorizationRestrictions =
      serde_json::from_value(no_restrictions_value).unwrap();
    assert_eq!(no_restrictions_response.version(), 1);
    assert_eq!(no_restrictions_response.htsget_auth().len(), 1);
    assert_eq!(
      no_restrictions_response.htsget_auth()[0].path(),
      "/path/to/file"
    );
    assert!(
      no_restrictions_response.htsget_auth()[0]
        .reference_names()
        .is_none()
    );
  }

  #[test]
  fn test_reference_name_restriction_builder() {
    let restriction = ReferenceNameRestrictionBuilder::default()
      .name("chr1")
      .format(Format::Bam)
      .start(3000)
      .end(2000)
      .build();
    assert!(restriction.is_err());

    let restriction = ReferenceNameRestrictionBuilder::default()
      .format(Format::Bam)
      .start(2000)
      .end(3000)
      .build();
    assert!(restriction.is_err());
  }

  #[test]
  fn test_authorization_rule_builder() {
    let rule = AuthorizationRuleBuilder::default()
      .path("/sample1")
      .build()
      .unwrap();
    assert_eq!(rule.path(), "/sample1");
    assert!(rule.reference_names().is_none());
    let rule = AuthorizationRuleBuilder::default().build();
    assert!(rule.is_err());
  }

  #[test]
  fn test_authorization_restrictions_builder() {
    let rule = AuthorizationRestrictionsBuilder::default()
      .version(0)
      .build();
    assert!(rule.is_err());
    let rule = AuthorizationRestrictionsBuilder::default()
      .version(1)
      .build();
    assert!(rule.is_err());
  }
}
