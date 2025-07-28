//! Authorization response data structures for JWT authorization.
//!
//! This module provides data structures for parsing and validating authorization
//! responses from external authorization services.

use crate::error;
use crate::error::Error::ValidationError;
use crate::types::{Format, Interval};
use serde::{Deserialize, Serialize};

/// Authorization response from external authorization service.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationResponse {
  version: u32,
  #[serde(rename = "htsgetAuth")]
  htsget_auth: Vec<AuthorizationRule>,
}

/// Individual authorization rule defining access permissions.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationRule {
  path: String,
  #[serde(rename = "referenceNames")]
  reference_names: Option<Vec<ReferenceNameRestriction>>,
}

/// Restriction on genomic reference names and coordinate ranges.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceNameRestriction {
  name: String,
  format: Option<Format>,
  #[serde(flatten)]
  interval: Interval,
}

impl AuthorizationResponse {
  /// Create a new authorization response.
  pub fn new(version: u32, htsget_auth: Vec<AuthorizationRule>) -> Self {
    Self {
      version,
      htsget_auth,
    }
  }

  /// Get the version of the authorization response format.
  pub fn version(&self) -> u32 {
    self.version
  }

  /// Get the authorization rules.
  pub fn htsget_auth(&self) -> &[AuthorizationRule] {
    &self.htsget_auth
  }

  /// Validate the authorization response structure.
  pub fn validate(&self) -> error::Result<()> {
    if self.version != 1 {
      return Err(ValidationError(format!(
        "invalid version: expected 1, got {}",
        self.version
      )));
    }

    if self.htsget_auth.is_empty() {
      return Err(ValidationError(
        "authorization response must contain at least one rule".to_string(),
      ));
    }

    self.htsget_auth.iter().try_for_each(|rule| rule.validate())
  }
}

impl AuthorizationRule {
  /// Create a new authorization rule.
  pub fn new(path: String, reference_names: Option<Vec<ReferenceNameRestriction>>) -> Self {
    Self {
      path,
      reference_names,
    }
  }

  /// Get the file path pattern that this rule allows access to.
  pub fn path(&self) -> &str {
    &self.path
  }

  /// Get the optional restrictions on reference names and genomic coordinates.
  pub fn reference_names(&self) -> Option<&[ReferenceNameRestriction]> {
    self.reference_names.as_deref()
  }

  /// Validate the authorization rule.
  pub fn validate(&self) -> error::Result<()> {
    if self.path.is_empty() {
      return Err(ValidationError("path cannot be empty".to_string()));
    }

    self
      .reference_names
      .iter()
      .try_for_each(|reference_name| reference_name.iter().try_for_each(|name| name.validate()))
  }
}

impl ReferenceNameRestriction {
  /// Create a new reference name restriction.
  pub fn new(name: String, format: Option<Format>, interval: Interval) -> Self {
    Self {
      name,
      format,
      interval,
    }
  }

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

  /// Validate the reference name restriction.
  pub fn validate(&self) -> error::Result<()> {
    if self.name.is_empty() {
      return Err(ValidationError("name cannot be empty".to_string()));
    }

    self.interval.validate()
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
    let response: AuthorizationResponse = serde_json::from_value(json_value).unwrap();

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
    let no_restrictions_response: AuthorizationResponse =
      serde_json::from_value(no_restrictions_value).unwrap();
    assert_eq!(no_restrictions_response.version(), 1);
    assert_eq!(no_restrictions_response.htsget_auth().len(), 1);
    assert_eq!(
      no_restrictions_response.htsget_auth()[0].path(),
      "/path/to/file"
    );
    assert!(no_restrictions_response.htsget_auth()[0]
      .reference_names()
      .is_none());
  }

  #[test]
  fn test_authorization_response_validation_success() {
    let response = example_authorization_response();

    assert!(response.validate().is_ok());
  }

  #[test]
  fn test_authorization_response_validation_invalid_version() {
    let response = AuthorizationResponse::new(
      2,
      vec![AuthorizationRule::new("/path/to/file".to_string(), None)],
    );

    let result = response.validate();
    assert!(result.is_err());
  }

  #[test]
  fn test_authorization_response_validation_empty_rules() {
    let response = AuthorizationResponse::new(1, vec![]);

    let result = response.validate();
    assert!(result.is_err());
  }

  #[test]
  fn test_authorization_rule_validation_empty_path() {
    let rule = AuthorizationRule::new("".to_string(), None);

    let result = rule.validate();
    assert!(result.is_err());
  }

  #[test]
  fn test_reference_name_restriction_validation_empty_name() {
    let restriction =
      ReferenceNameRestriction::new("".to_string(), None, Interval::new(None, None));

    let result = restriction.validate();
    assert!(result.is_err());
  }

  fn example_authorization_response() -> AuthorizationResponse {
    AuthorizationResponse::new(
      1,
      vec![AuthorizationRule::new(
        "/path/to/file".to_string(),
        Some(vec![ReferenceNameRestriction::new(
          "chr1".to_string(),
          Some(Format::Bam),
          Interval::new(Some(1000), Some(2000)),
        )]),
      )],
    )
  }
}
