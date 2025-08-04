//! Authorization response data structures for JWT authorization.
//!
//! This module provides data structures for parsing and validating authorization
//! responses from external authorization services.
//!

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
  #[serde(rename = "referenceNames")]
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
  format: Option<Format>,
  /// The interval to allow. Allows all intervals if unspecified.
  #[serde(flatten)]
  interval: Interval,
}

impl AuthorizationRestrictions {
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
}
