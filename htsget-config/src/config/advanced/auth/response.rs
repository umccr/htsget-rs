//! Authorization response data structures for JWT authorization.
//!
//! This module provides data structures for parsing and validating authorization
//! responses from external authorization services.
//!

use crate::config::location::{Location, Locations};
use crate::error::Error::BuilderError;
use crate::error::{Error, Result};
use crate::types::{Format, Interval};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Authorization restrictions from an external authorization service.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationRule {
  /// The location that the authorization applies to.
  location: Location,
  /// The reference name restrictions to apply to this path.
  #[serde(skip_serializing_if = "Option::is_none")]
  rules: Option<Vec<ReferenceNameRestriction>>,
}

/// Restriction on genomic reference names and coordinate ranges.
#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceNameRestriction {
  /// The reference name to allow. Allows all reference names if unspecified.
  #[serde(rename = "referenceName", skip_serializing_if = "Option::is_none")]
  reference_name: Option<String>,
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

  /// Get the authorization rules as an owned vec.
  pub fn into_rules(self) -> Vec<AuthorizationRule> {
    self.htsget_auth
  }

  /// Get the possible locations from the remote authorization service that would be valid to
  /// search with. I.e. locations with backends that are not defaulted.
  pub fn into_remote_locations(self) -> Locations {
    self
      .into_rules()
      .into_iter()
      .flat_map(|r| {
        let location = r.into_location();
        if location.backend().is_defaulted() {
          None
        } else {
          Some(location)
        }
      })
      .collect::<Vec<_>>()
      .into()
  }
}

impl AuthorizationRule {
  /// The location of the rule.
  pub fn location(&self) -> &Location {
    &self.location
  }

  /// Get the owned location.
  pub fn into_location(self) -> Location {
    self.location
  }

  /// Get the optional rules on reference names and genomic coordinates.
  pub fn rules(&self) -> Option<&[ReferenceNameRestriction]> {
    self.rules.as_deref()
  }

  /// Get the optional rules on reference names and genomic coordinates as a mutable reference.
  pub fn rules_mut(&mut self) -> Option<&mut [ReferenceNameRestriction]> {
    self.rules.as_deref_mut()
  }
}

impl ReferenceNameRestriction {
  /// Get the name of the reference sequence.
  pub fn reference_name(&self) -> Option<&str> {
    self.reference_name.as_deref()
  }

  /// Get the optional format restriction.
  pub fn format(&self) -> Option<Format> {
    self.format
  }

  /// Get the interval to allow.
  pub fn interval(&self) -> &Interval {
    &self.interval
  }

  /// Set the interval.
  pub fn set_interval(&mut self, interval: Interval) {
    self.interval = interval;
  }
}

/// Builder for `AuthorizationRestrictions`.
#[derive(JsonSchema, Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationRestrictionsBuilder {
  /// The version of the schema.
  #[validate(range(min = 1))]
  version: Option<u32>,
  /// The authorization rules.
  #[serde(rename = "htsgetAuth")]
  #[validate(length(min = 1))]
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
#[derive(JsonSchema, Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationRuleBuilder {
  /// The location that the authorization applies to.
  location: Option<Location>,
  /// The reference name restrictions to apply to this path.
  #[serde(skip_serializing_if = "Vec::is_empty")]
  reference_names: Vec<ReferenceNameRestriction>,
}

impl AuthorizationRuleBuilder {
  /// Set the location for this rule.
  pub fn location(mut self, location_either: Location) -> Self {
    self.location = Some(location_either);
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
    let location = self
      .location
      .ok_or_else(|| BuilderError("location not set".to_string()))?;
    if location
      .as_simple()
      .is_ok_and(|simple| simple.prefix_or_id().is_none())
    {
      return Err(BuilderError("A prefix or id must be set".to_string()))?;
    }

    Ok(AuthorizationRule {
      location,
      rules: if self.reference_names.is_empty() {
        None
      } else {
        Some(self.reference_names)
      },
    })
  }
}

/// Builder for `ReferenceNameRestriction`.
#[derive(JsonSchema, Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceNameRestrictionBuilder {
  /// The reference name to allow. Allows all reference names if unspecified.
  #[serde(rename = "referenceName", skip_serializing_if = "Option::is_none")]
  name: Option<String>,
  /// The format to allow. Allows all formats if unspecified.
  #[serde(skip_serializing_if = "Option::is_none")]
  format: Option<Format>,
  /// The start interval to allow. Allows any start interval if unspecified.
  start: Option<u32>,
  /// The end interval to allow. Allows any end interval if unspecified.
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
      reference_name: self.name,
      format: self.format,
      interval: Interval::new(self.start, self.end),
    })
  }
}

impl TryFrom<AuthorizationRestrictionsBuilder> for AuthorizationRestrictions {
  type Error = Error;

  fn try_from(builder: AuthorizationRestrictionsBuilder) -> Result<Self> {
    builder.build()
  }
}

impl TryFrom<AuthorizationRuleBuilder> for AuthorizationRule {
  type Error = Error;

  fn try_from(builder: AuthorizationRuleBuilder) -> Result<Self> {
    builder.build()
  }
}

impl TryFrom<ReferenceNameRestrictionBuilder> for ReferenceNameRestriction {
  type Error = Error;

  fn try_from(builder: ReferenceNameRestrictionBuilder) -> Result<Self> {
    builder.build()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::location::{PrefixOrId, SimpleLocation};
  use serde_json;
  use std::result;

  #[test]
  fn test_authorization_response_deserialization() {
    let json_value = serde_json::json!({
      "version": 1,
      "htsgetAuth": [
        {
          "location": {
            "id": "HG00096",
            "backend": "s3://umccr-10g-data-dev/HG00096/HG00096",
          },
          "rules": [
            {
              "format": "BAM"
            }
          ]
        }
      ]
    });
    let response: result::Result<AuthorizationRestrictions, _> = serde_json::from_value(json_value);

    println!("{:#?}", response);

    //
    // assert_eq!(response.version(), 1);
    // assert_eq!(response.htsget_auth().len(), 1);
    // assert_eq!(
    //   response.htsget_auth()[0]
    //     .location()
    //     .as_simple()
    //     .unwrap()
    //     .prefix_or_id()
    //     .unwrap()
    //     .as_id()
    //     .unwrap(),
    //   "path/to/file"
    // );
    //
    // let restrictions = response.htsget_auth()[0].rules().unwrap();
    // assert_eq!(restrictions.len(), 1);
    // assert_eq!(restrictions[0].reference_name(), Some("chr1"));
    // assert_eq!(restrictions[0].interval().start(), Some(1000));
    // assert_eq!(restrictions[0].interval().end(), Some(2000));
    // assert_eq!(restrictions[0].format(), Some(Format::Bam));
    //
    // let no_restrictions_value = serde_json::json!({
    //   "version": 1,
    //   "htsgetAuth": [{
    //     "location": {
    //       "id": "path/to/file"
    //     }
    //   }]
    // });
    // let no_restrictions_response: AuthorizationRestrictions =
    //   serde_json::from_value(no_restrictions_value).unwrap();
    // assert_eq!(no_restrictions_response.version(), 1);
    // assert_eq!(no_restrictions_response.htsget_auth().len(), 1);
    // assert_eq!(
    //   no_restrictions_response.htsget_auth()[0]
    //     .location()
    //     .as_simple()
    //     .unwrap()
    //     .prefix_or_id()
    //     .unwrap()
    //     .as_id()
    //     .unwrap(),
    //   "path/to/file"
    // );
    // assert!(no_restrictions_response.htsget_auth()[0].rules().is_none());
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
    assert!(restriction.is_ok());
  }

  #[test]
  fn test_authorization_rule_builder() {
    let rule = AuthorizationRuleBuilder::default()
      .location(Location::Simple(Box::new(SimpleLocation::new(
        Default::default(),
        "".to_string(),
        Some(PrefixOrId::Id("sample1".to_string())),
      ))))
      .build()
      .unwrap();
    assert_eq!(
      rule
        .location()
        .as_simple()
        .unwrap()
        .prefix_or_id()
        .unwrap()
        .as_id()
        .unwrap(),
      "sample1"
    );
    assert!(rule.rules().is_none());

    let rule = AuthorizationRuleBuilder::default().build();
    assert!(rule.is_err());

    let rule = AuthorizationRuleBuilder::default()
      .location(Location::Simple(Box::new(SimpleLocation::new(
        Default::default(),
        "".to_string(),
        None,
      ))))
      .build();
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
