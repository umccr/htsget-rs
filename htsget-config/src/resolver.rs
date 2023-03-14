use regex::{Error, Regex};
use serde::{Deserialize, Serialize};
use serde_with::with_prefix;
use std::collections::HashSet;
use tracing::instrument;

use crate::storage::Storage;
use crate::Format::{Bam, Bcf, Cram, Vcf};
use crate::{Class, Fields, Format, Interval, Query, TaggedTypeAll, Tags};

/// Represents an id storage, which matches the id, replacing the match in the substitution text.
pub trait Resolver {
  /// Resolve the id, returning the substituted string if there is a match.
  fn resolve_id(&self, query: &Query) -> Option<String>;
}

/// Determines whether the query matches for use with the storage.
pub trait QueryAllowed {
  /// Does this query match.
  fn query_allowed(&self, query: &Query) -> bool;
}

/// A regex storage is a storage that matches ids using Regex.
#[derive(Serialize, Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RegexResolver {
  #[serde(with = "serde_regex")]
  regex: Regex,
  // Todo: should match guard be allowed as variables inside the substitution string?
  substitution_string: String,
  storage: Storage,
  allow_guard: AllowGuard,
}

with_prefix!(allow_interval_prefix "allow_interval_");

/// A query guard represents query parameters that can be allowed to storage for a given query.
#[derive(Serialize, Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AllowGuard {
  allow_reference_names: ReferenceNames,
  allow_fields: Fields,
  allow_tags: Tags,
  allow_formats: Vec<Format>,
  allow_classes: Vec<Class>,
  #[serde(flatten, with = "allow_interval_prefix")]
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

impl QueryAllowed for ReferenceNames {
  fn query_allowed(&self, query: &Query) -> bool {
    match (self, &query.reference_name) {
      (ReferenceNames::Tagged(TaggedTypeAll::All), _) => true,
      (ReferenceNames::List(reference_names), Some(reference_name)) => {
        reference_names.contains(reference_name)
      }
      (ReferenceNames::List(_), None) => false,
    }
  }
}

impl QueryAllowed for Fields {
  fn query_allowed(&self, query: &Query) -> bool {
    match (self, &query.fields) {
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
    match (self, &query.tags) {
      (Tags::Tagged(TaggedTypeAll::All), _) => true,
      (Tags::List(self_tags), Tags::List(query_tags)) => self_tags.is_subset(query_tags),
      (Tags::List(_), Tags::Tagged(TaggedTypeAll::All)) => false,
    }
  }
}

impl QueryAllowed for AllowGuard {
  fn query_allowed(&self, query: &Query) -> bool {
    self.allow_formats.contains(&query.format)
      && self.allow_classes.contains(&query.class)
      && self
        .allow_interval
        .contains(query.interval.start.unwrap_or(u32::MIN))
      && self
        .allow_interval
        .contains(query.interval.end.unwrap_or(u32::MAX))
      && self.allow_reference_names.query_allowed(query)
      && self.allow_fields.query_allowed(query)
      && self.allow_tags.query_allowed(query)
  }
}

impl Default for RegexResolver {
  fn default() -> Self {
    Self::new(Storage::default(), ".*", "$0", AllowGuard::default())
      .expect("expected valid storage")
  }
}

impl RegexResolver {
  /// Create a new regex storage.
  pub fn new(
    storage: Storage,
    regex: &str,
    replacement_string: &str,
    allow_guard: AllowGuard,
  ) -> Result<Self, Error> {
    Ok(Self {
      regex: Regex::new(regex)?,
      substitution_string: replacement_string.to_string(),
      storage,
      allow_guard,
    })
  }

  /// Get the regex.
  pub fn regex(&self) -> &Regex {
    &self.regex
  }

  /// Get the substitution string.
  pub fn substitution_string(&self) -> &str {
    &self.substitution_string
  }

  /// Get the query guard.
  pub fn allow_guard(&self) -> &AllowGuard {
    &self.allow_guard
  }

  /// Get the storage backend.
  pub fn storage(&self) -> &Storage {
    &self.storage
  }

  /// Get allow formats.
  pub fn allow_formats(&self) -> &[Format] {
    self.allow_guard.allow_formats()
  }

  /// Get allow classes.
  pub fn allow_classes(&self) -> &[Class] {
    self.allow_guard.allow_classes()
  }

  /// Get allow interval.
  pub fn allow_interval(&self) -> Interval {
    self.allow_guard.allow_interval
  }

  /// Get allow reference names.
  pub fn allow_reference_names(&self) -> &ReferenceNames {
    &self.allow_guard.allow_reference_names
  }

  /// Get allow fields.
  pub fn allow_fields(&self) -> &Fields {
    &self.allow_guard.allow_fields
  }

  /// Get allow tags.
  pub fn allow_tags(&self) -> &Tags {
    &self.allow_guard.allow_tags
  }
}

impl Resolver for RegexResolver {
  #[instrument(level = "trace", skip(self), ret)]
  fn resolve_id(&self, query: &Query) -> Option<String> {
    if self.regex.is_match(&query.id) && self.allow_guard.query_allowed(query) {
      Some(
        self
          .regex
          .replace(&query.id, &self.substitution_string)
          .to_string(),
      )
    } else {
      None
    }
  }
}

#[cfg(test)]
pub mod tests {
  use super::*;
  use crate::config::tests::{test_config_from_env, test_config_from_file};
  #[cfg(feature = "s3-storage")]
  use crate::storage::s3::S3Storage;

  #[test]
  fn resolver_resolve_id() {
    let resolver =
      RegexResolver::new(Storage::default(), ".*", "$0-test", AllowGuard::default()).unwrap();
    assert_eq!(
      resolver.resolve_id(&Query::new("id", Bam)).unwrap(),
      "id-test"
    );
  }

  #[test]
  fn config_resolvers_file() {
    test_config_from_file(
      r#"
            [[resolvers]]
            regex = "regex"
        "#,
      |config| {
        assert_eq!(
          config.resolvers().first().unwrap().regex().as_str(),
          "regex"
        );
      },
    );
  }

  #[test]
  fn config_resolvers_guard_file() {
    test_config_from_file(
      r#"
            [[resolvers]]
            regex = "regex"

            [resolvers.allow_guard]
            allow_formats = ["BAM"]
        "#,
      |config| {
        assert_eq!(
          config.resolvers().first().unwrap().allow_formats(),
          &vec![Bam]
        );
      },
    );
  }

  #[test]
  fn config_resolvers_env() {
    test_config_from_env(vec![("HTSGET_RESOLVERS", "[{regex=regex}]")], |config| {
      assert_eq!(
        config.resolvers().first().unwrap().regex().as_str(),
        "regex"
      );
    });
  }

  #[cfg(feature = "s3-storage")]
  #[test]
  fn config_resolvers_all_options_env() {
    test_config_from_env(
      vec![(
        "HTSGET_RESOLVERS",
        "[{ regex=regex, substitution_string=substitution_string, \
        storage={ bucket=bucket }, \
        allow_guard={ allow_reference_names=[chr1], allow_fields=[QNAME], allow_tags=[RG], \
        allow_formats=[BAM], allow_classes=[body], allow_interval_start=100, \
        allow_interval_end=1000 } }]",
      )],
      |config| {
        let storage = Storage::S3 {
          s3_storage: S3Storage::new("bucket".to_string()),
        };
        let allow_guard = AllowGuard::new(
          ReferenceNames::List(HashSet::from_iter(vec!["chr1".to_string()])),
          Fields::List(HashSet::from_iter(vec!["QNAME".to_string()])),
          Tags::List(HashSet::from_iter(vec!["RG".to_string()])),
          vec![Bam],
          vec![Class::Body],
          Interval {
            start: Some(100),
            end: Some(1000),
          },
        );
        let resolver = config.resolvers().first().unwrap();

        assert_eq!(resolver.regex().to_string(), "regex");
        assert_eq!(resolver.substitution_string(), "substitution_string");
        assert_eq!(resolver.storage(), &storage);
        assert_eq!(resolver.allow_guard(), &allow_guard);
      },
    );
  }
}
