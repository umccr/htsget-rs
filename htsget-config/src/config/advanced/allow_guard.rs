//! Allow guard configuration.
//!

use crate::types::{Class, Fields, Format, Interval, TaggedTypeAll, Tags};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A query guard represents query parameters that can be allowed to storage for a given query.
#[derive(Serialize, Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AllowGuard {
  allow_reference_names: ReferenceNames,
  allow_fields: Fields,
  allow_tags: Tags,
  allow_formats: Vec<Format>,
  allow_classes: Vec<Class>,
  allow_interval: Interval,
}

/// Reference names that can be matched.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ReferenceNames {
  Tagged(TaggedTypeAll),
  List(HashSet<String>),
}

impl AllowGuard {
  /// Create a new allow guard.
  pub fn new(
    allow_reference_names: ReferenceNames,
    allow_fields: Fields,
    allow_tags: Tags,
    allow_formats: Vec<Format>,
    allow_classes: Vec<Class>,
    allow_interval: Interval,
  ) -> Self {
    Self {
      allow_reference_names,
      allow_fields,
      allow_tags,
      allow_formats,
      allow_classes,
      allow_interval,
    }
  }

  /// Get allow formats.
  pub fn allow_formats(&self) -> &[Format] {
    &self.allow_formats
  }

  /// Get allow classes.
  pub fn allow_classes(&self) -> &[Class] {
    &self.allow_classes
  }

  /// Get allow interval.
  pub fn allow_interval(&self) -> Interval {
    self.allow_interval
  }

  /// Get allow reference names.
  pub fn allow_reference_names(&self) -> &ReferenceNames {
    &self.allow_reference_names
  }

  /// Get allow fields.
  pub fn allow_fields(&self) -> &Fields {
    &self.allow_fields
  }

  /// Get allow tags.
  pub fn allow_tags(&self) -> &Tags {
    &self.allow_tags
  }
}
