use http::uri::Authority;
use regex::{Error, Regex};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::config::{default_localstorage_addr, default_path, default_serve_at};
use crate::regex_resolver::aws::S3Resolver;
use crate::Format::{Bam, Bcf, Cram, Vcf};
use crate::{Class, Fields, Format, Interval, NoTags, Query, Tags};
use crate::regex_resolver::ReferenceNames::All;

#[cfg(feature = "s3-storage")]
pub mod aws;

/// Represents an id resolver, which matches the id, replacing the match in the substitution text.
pub trait Resolver {
  /// Resolve the id, returning the substituted string if there is a match.
  fn resolve_id(&self, query: &Query) -> Option<String>;
}

/// Determines whether the query matches for use with the resolver.
pub trait QueryMatcher {
  /// Does this query match.
  fn query_matches(&self, query: &Query) -> bool;
}

/// Specify the storage type to use.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum StorageType {
  #[serde(alias = "url", alias = "URL")]
  Local(LocalResolver),
  #[cfg(feature = "s3-storage")]
  #[serde(alias = "s3")]
  S3(S3Resolver),
}

impl Default for StorageType {
  fn default() -> Self {
    Self::Local(LocalResolver::default())
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
  #[serde(alias = "http", alias = "HTTP")]
  Http,
  #[serde(alias = "https", alias = "HTTPS")]
  Https,
}

impl Default for Scheme {
  fn default() -> Self {
    Self::Http
  }
}

/// Configuration for the htsget server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct LocalResolver {
  scheme: Scheme,
  #[serde(with = "http_serde::authority")]
  authority: Authority,
  local_path: String,
  path_prefix: String
}

impl LocalResolver {
  pub fn scheme(&self) -> Scheme {
    self.scheme
  }

  pub fn authority(&self) -> &Authority {
    &self.authority
  }

  pub fn local_path(&self) -> &str {
    &self.local_path
  }

  pub fn path_prefix(&self) -> &str {
    &self.path_prefix
  }

  pub fn set_scheme(&mut self, scheme: Scheme) {
    self.scheme = scheme;
  }

  pub fn set_authority(&mut self, authority: Authority) {
    self.authority = authority;
  }

  pub fn set_local_path(&mut self, local_path: String) {
    self.local_path = local_path;
  }

  pub fn set_path_prefix(&mut self, path_prefix: String) {
    self.path_prefix = path_prefix;
  }
}

impl Default for LocalResolver {
  fn default() -> Self {
    Self {
      scheme: Scheme::default(),
      authority: Authority::from_static(default_localstorage_addr()),
      local_path: default_path().into(),
      path_prefix: default_serve_at().into(),
    }
  }
}

/// A regex resolver is a resolver that matches ids using Regex.
#[derive(Serialize, Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RegexResolver {
  #[serde(with = "serde_regex")]
  regex: Regex,
  // Todo: should match guard be allowed as variables inside the substitution string?
  substitution_string: String,
  guard: QueryGuard,
  storage_type: StorageType,
}

/// A query that can be matched with the regex resolver.
#[derive(Serialize, Clone, Debug, Deserialize)]
#[serde(default)]
pub struct QueryGuard {
  allow_formats: Vec<Format>,
  allow_classes: Vec<Class>,
  allow_reference_names: ReferenceNames,
  allow_interval: Interval,
  allow_fields: Fields,
  allow_tags: Tags,
}

/// Referneces names that can be matched.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ReferenceNames {
  All,
  #[serde(with = "serde_regex")]
  Some(Regex)
}

impl QueryGuard {
  pub fn allow_formats(&self) -> &[Format] {
    &self.allow_formats
  }

  pub fn allow_classes(&self) -> &[Class] {
    &self.allow_classes
  }

  pub fn allow_reference_names(&self) -> &ReferenceNames {
    &self.allow_reference_names
  }

  pub fn allow_interval(&self) -> Interval {
    self.allow_interval
  }

  pub fn allow_fields(&self) -> &Fields {
    &self.allow_fields
  }

  pub fn allow_tags(&self) -> &Tags {
    &self.allow_tags
  }
}

impl Default for QueryGuard {
  fn default() -> Self {
    Self {
      allow_formats: vec![Bam, Cram, Vcf, Bcf],
      allow_classes: vec![Class::Body, Class::Header],
      allow_reference_names: All,
      allow_interval: Default::default(),
      allow_fields: Fields::All,
      allow_tags: Tags::All,
    }
  }
}

impl QueryMatcher for ReferenceNames {
  fn query_matches(&self, query: &Query) -> bool {
    match (self, &query.reference_name) {
      (ReferenceNames::All, _) => true,
      (ReferenceNames::Some(regex), Some(reference_name)) => regex.is_match(reference_name),
      (ReferenceNames::Some(_), None) => false,
    }
  }
}

impl QueryMatcher for Fields {
  fn query_matches(&self, query: &Query) -> bool {
    match (self, &query.fields) {
      (Fields::All, _) => true,
      (Fields::List(self_fields), Fields::List(query_fields)) => self_fields.is_subset(query_fields),
      (Fields::List(_), Fields::All) => false,
    }
  }
}

impl QueryMatcher for Tags {
  fn query_matches(&self, query: &Query) -> bool {
    match (self, &query.tags) {
      (Tags::All, _) => true,
      (Tags::List(self_tags), Tags::List(query_tags)) => self_tags.is_subset(query_tags),
      (Tags::List(_), Tags::All) => false,
    }
  }
}

impl QueryMatcher for QueryGuard {
  fn query_matches(&self, query: &Query) -> bool {
    self.allow_formats.contains(&query.format)
        && self.allow_classes.contains(&query.class)
        && self.allow_reference_names.query_matches(query)
        && self
          .allow_interval
          .contains(query.interval.start.unwrap_or(u32::MIN))
        && self
          .allow_interval
          .contains(query.interval.end.unwrap_or(u32::MAX))
        && self.allow_fields.query_matches(query)
        && self.allow_tags.query_matches(query)
  }
}

impl Default for RegexResolver {
  fn default() -> Self {
    Self::new(StorageType::default(), ".*", "$0", QueryGuard::default())
      .expect("expected valid resolver")
  }
}

impl RegexResolver {
  /// Create a new regex resolver.
  pub fn new(
    storage_type: StorageType,
    regex: &str,
    replacement_string: &str,
    guard: QueryGuard,
  ) -> Result<Self, Error> {
    Ok(Self {
      regex: Regex::new(regex)?,
      substitution_string: replacement_string.to_string(),
      storage_type,
      guard,
    })
  }

  pub fn regex(&self) -> &Regex {
    &self.regex
  }

  pub fn substitution_string(&self) -> &str {
    &self.substitution_string
  }

  pub fn guard(&self) -> &QueryGuard {
    &self.guard
  }

  pub fn storage_type(&self) -> &StorageType {
    &self.storage_type
  }

  pub fn allow_formats(&self) -> &[Format] {
    self.guard.allow_formats()
  }

  pub fn allow_classes(&self) -> &[Class] {
    self.guard.allow_classes()
  }

  pub fn allow_reference_names(&self) -> &ReferenceNames {
    &self.guard.allow_reference_names
  }

  pub fn allow_interval(&self) -> Interval {
    self.guard.allow_interval
  }

  pub fn allow_fields(&self) -> &Fields {
    &self.guard.allow_fields
  }

  pub fn allow_tags(&self) -> &Tags {
    &self.guard.allow_tags
  }
}

impl Resolver for RegexResolver {
  #[instrument(level = "trace", skip(self), ret)]
  fn resolve_id(&self, query: &Query) -> Option<String> {
    if self.regex.is_match(&query.id) && self.guard.query_matches(query) {
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

  #[test]
  fn resolver_resolve_id() {
    let mut resolver = RegexResolver::new(
      StorageType::default(),
      ".*",
      "$0-test",
      QueryGuard::default(),
    )
    .unwrap();
    assert_eq!(
      resolver.resolve_id(&Query::new("id", Bam)).unwrap(),
      "id-test"
    );
  }
}
