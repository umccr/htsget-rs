//! Allow guard configuration.
//!

use crate::types::Format::{Bam, Bcf, Cram, Vcf};
use crate::types::{Class, Fields, Format, Interval, Query, TaggedTypeAll, Tags};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Determines whether the query matches for use with the storage.
pub trait QueryAllowed {
  /// Does this query match.
  fn query_allowed(&self, query: &Query) -> bool;
}

/// A query guard represents query parameters that can be allowed to storage for a given query.
#[derive(Serialize, Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct AllowGuard {
  allow_reference_names: ReferenceNames,
  allow_fields: Fields,
  allow_tags: Tags,
  allow_formats: Vec<Format>,
  allow_classes: Vec<Class>,
  allow_interval: Interval,
}

impl Default for AllowGuard {
  fn default() -> Self {
    Self {
      allow_formats: vec![Bam, Cram, Vcf, Bcf],
      allow_classes: vec![Class::Body, Class::Header],
      allow_interval: Default::default(),
      allow_reference_names: ReferenceNames::Tagged(TaggedTypeAll::All),
      allow_fields: Fields::Tagged(TaggedTypeAll::All),
      allow_tags: Tags::Tagged(TaggedTypeAll::All),
    }
  }
}

/// Reference names that can be matched.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
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

impl QueryAllowed for ReferenceNames {
  fn query_allowed(&self, query: &Query) -> bool {
    match (self, &query.reference_name()) {
      (ReferenceNames::Tagged(TaggedTypeAll::All), _) => true,
      (ReferenceNames::List(reference_names), Some(reference_name)) => {
        reference_names.contains(*reference_name)
      }
      (ReferenceNames::List(_), None) => false,
    }
  }
}

impl QueryAllowed for Fields {
  fn query_allowed(&self, query: &Query) -> bool {
    match (self, &query.fields()) {
      (Fields::Tagged(TaggedTypeAll::All), _) => true,
      (Fields::List(self_fields), Fields::List(query_fields)) => {
        self_fields.is_subset(query_fields)
      }
      (Fields::List(_), Fields::Tagged(TaggedTypeAll::All)) => false,
    }
  }
}

impl QueryAllowed for Tags {
  fn query_allowed(&self, query: &Query) -> bool {
    match (self, &query.tags()) {
      (Tags::Tagged(TaggedTypeAll::All), _) => true,
      (Tags::List(self_tags), Tags::List(query_tags)) => self_tags.is_subset(query_tags),
      (Tags::List(_), Tags::Tagged(TaggedTypeAll::All)) => false,
    }
  }
}

impl QueryAllowed for AllowGuard {
  fn query_allowed(&self, query: &Query) -> bool {
    self.allow_formats().contains(&query.format())
      && self.allow_classes().contains(&query.class())
      && self
        .allow_interval()
        .contains(query.interval().start().unwrap_or(u32::MIN))
      && self
        .allow_interval()
        .contains(query.interval().end().unwrap_or(u32::MAX))
      && self.allow_reference_names().query_allowed(query)
      && self.allow_fields().query_allowed(query)
      && self.allow_tags().query_allowed(query)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::tests::test_serialize_and_deserialize;
  #[cfg(feature = "s3-storage")]
  use crate::config::Config;
  use crate::types::Class::Header;

  #[test]
  fn allow_reference_names_all() {
    test_serialize_and_deserialize(
      "allow_reference_names = \"All\"",
      AllowGuard {
        allow_reference_names: ReferenceNames::Tagged(TaggedTypeAll::All),
        ..Default::default()
      },
      |result| result,
    );
  }

  #[test]
  fn allow_reference_names_list() {
    test_serialize_and_deserialize(
      "allow_reference_names = [\"chr1\"]",
      AllowGuard {
        allow_reference_names: ReferenceNames::List(HashSet::from_iter(vec!["chr1".to_string()])),
        ..Default::default()
      },
      |result| result,
    );
  }

  #[test]
  fn allow_fields_all() {
    test_serialize_and_deserialize(
      "allow_fields = \"All\"",
      AllowGuard {
        allow_fields: Fields::Tagged(TaggedTypeAll::All),
        ..Default::default()
      },
      |result| result,
    );
  }

  #[test]
  fn allow_fields_list() {
    test_serialize_and_deserialize(
      "allow_fields = [\"field\"]",
      AllowGuard {
        allow_fields: Fields::List(HashSet::from_iter(vec!["field".to_string()])),
        ..Default::default()
      },
      |result| result,
    );
  }

  #[test]
  fn allow_tags_all() {
    test_serialize_and_deserialize(
      "allow_tags = \"All\"",
      AllowGuard {
        allow_tags: Tags::Tagged(TaggedTypeAll::All),
        ..Default::default()
      },
      |result| result,
    );
  }

  #[test]
  fn allow_tags_list() {
    test_serialize_and_deserialize(
      "allow_tags = [\"tag\"]",
      AllowGuard {
        allow_tags: Tags::List(HashSet::from_iter(vec!["tag".to_string()])),
        ..Default::default()
      },
      |result| result,
    );
  }

  #[test]
  fn allow_formats() {
    test_serialize_and_deserialize(
      "allow_formats = [\"BAM\"]",
      AllowGuard {
        allow_formats: vec![Bam],
        ..Default::default()
      },
      |result| result,
    );
  }

  #[test]
  fn allow_classes() {
    test_serialize_and_deserialize(
      "allow_classes = [\"Header\"]",
      AllowGuard {
        allow_classes: vec![Header],
        ..Default::default()
      },
      |result| result,
    );
  }

  #[test]
  fn allow_interval() {
    test_serialize_and_deserialize(
      r#"
      allow_interval.start = 0
      allow_interval.end = 100
      "#,
      AllowGuard {
        allow_interval: Interval::new(Some(0), Some(100)),
        ..Default::default()
      },
      |result| result,
    );
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn allow_guard() {
    test_serialize_and_deserialize(
      r#"
      [[locations]]
      regex = ".*"
      substitution_string = "$0"

      backend.kind = "S3"
      backend.bucket = "bucket"

      guard.allow_reference_names = ["chr1"]
      guard.allow_interval.start = 100
      guard.allow_interval.end = 1000
      "#,
      AllowGuard {
        allow_reference_names: ReferenceNames::List(HashSet::from_iter(vec!["chr1".to_string()])),
        allow_interval: Interval::new(Some(100), Some(1000)),
        ..Default::default()
      },
      |result: Config| {
        let location = result.locations.into_inner();
        let location = location[0].as_regex().unwrap();

        AllowGuard {
          allow_reference_names: location.guard().unwrap().allow_reference_names.clone(),
          allow_interval: location.guard().unwrap().allow_interval,
          ..Default::default()
        }
      },
    );
  }

  #[test]
  fn query_allowed() {
    let guard = AllowGuard {
      allow_formats: vec![Bam],
      allow_classes: vec![Header],
      allow_interval: Interval::new(Some(0), Some(100)),
      ..Default::default()
    };

    let query = Query::new_with_default_request("", Bam);
    assert!(!guard.query_allowed(&query));
    assert!(!guard.query_allowed(&query.clone().with_format(Cram)));

    assert!(guard.query_allowed(&query.clone().with_class(Header).with_start(1).with_end(50)));
    assert!(guard.query_allowed(
      &query
        .clone()
        .with_class(Header)
        .with_start(1)
        .with_end(50)
        .with_tags(Tags::List(HashSet::from_iter(vec!["tag".to_string()])))
    ));
    assert!(guard.query_allowed(
      &query
        .clone()
        .with_class(Header)
        .with_start(1)
        .with_end(50)
        .with_fields(Fields::List(HashSet::from_iter(vec!["field".to_string()])))
    ));

    assert!(!guard.query_allowed(
      &query
        .clone()
        .with_class(Header)
        .with_start(1)
        .with_end(1000)
    ));
  }
}
