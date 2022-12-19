use http::uri::Authority;
use regex::{Error, Regex};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::config::{default_localstorage_addr, default_serve_at};
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
  Url(UrlResolver),
  #[cfg(feature = "s3-storage")]
  #[serde(alias = "s3")]
  S3(S3Resolver),
}

impl Default for StorageType {
  fn default() -> Self {
    Self::Url(UrlResolver::default())
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
pub struct UrlResolver {
  scheme: Scheme,
  #[serde(with = "http_serde::authority")]
  authority: Authority,
  path: String,
}

impl UrlResolver {
  pub fn scheme(&self) -> Scheme {
    self.scheme
  }

  pub fn authority(&self) -> &Authority {
    &self.authority
  }

  pub fn path(&self) -> &str {
    &self.path
  }
}

impl Default for UrlResolver {
  fn default() -> Self {
    Self {
      scheme: Scheme::default(),
      authority: Authority::from_static(default_localstorage_addr()),
      path: default_serve_at().to_string(),
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
  allowed_formats: Vec<Format>,
  allowed_classes: Vec<Class>,
  allowed_reference_names: ReferenceNames,
  allowed_interval: Interval,
  allowed_fields: Fields,
  allowed_tags: Tags,
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
  pub fn allowed_formats(&self) -> &[Format] {
    &self.allowed_formats
  }

  pub fn allowed_classes(&self) -> &[Class] {
    &self.allowed_classes
  }

  pub fn allowed_reference_names(&self) -> &ReferenceNames {
    &self.allowed_reference_names
  }

  pub fn allowed_interval(&self) -> Interval {
    self.allowed_interval
  }

  pub fn allowed_fields(&self) -> &Fields {
    &self.allowed_fields
  }

  pub fn allowed_tags(&self) -> &Tags {
    &self.allowed_tags
  }
}

impl Default for QueryGuard {
  fn default() -> Self {
    Self {
      allowed_formats: vec![Bam, Cram, Vcf, Bcf],
      allowed_classes: vec![Class::Body, Class::Header],
      allowed_reference_names: All,
      allowed_interval: Default::default(),
      allowed_fields: Fields::All,
      allowed_tags: Tags::All,
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
    self.allowed_formats.contains(&query.format)
        && self.allowed_classes.contains(&query.class)
        && self.allowed_reference_names.query_matches(query)
        && self
          .allowed_interval
          .contains(query.interval.start.unwrap_or(u32::MIN))
        && self
          .allowed_interval
          .contains(query.interval.end.unwrap_or(u32::MAX))
        && self.allowed_fields.query_matches(query)
        && self.allowed_tags.query_matches(query)
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

  pub fn allowed_formats(&self) -> &[Format] {
    self.guard.allowed_formats()
  }

  pub fn allowed_classes(&self) -> &[Class] {
    self.guard.allowed_classes()
  }

  pub fn allowed_reference_names(&self) -> &ReferenceNames {
    &self.guard.allowed_reference_names
  }

  pub fn allowed_interval(&self) -> Interval {
    self.guard.allowed_interval
  }

  pub fn allowed_fields(&self) -> &Fields {
    &self.guard.allowed_fields
  }

  pub fn allowed_tags(&self) -> &Tags {
    &self.guard.allowed_tags
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

// impl<'a, I> Resolver for I
// where
//   I: Iterator<Item = &'a RegexResolver>,
// {
//   fn resolve_id(&self, query: &Query) -> Option<String> {
//     self.find_map(|resolver| resolver.resolve_id(query))
//   }
// }

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
