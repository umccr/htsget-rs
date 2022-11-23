use regex::{Error, Regex};
use serde::Deserialize;
use tracing::instrument;

use crate::Format::{Bam, Bcf, Cram, Vcf};
use crate::{Class, Fields, Format, Interval, NoTags, Query, Tags};
use crate::config::StorageType;

/// Represents an id resolver, which matches the id, replacing the match in the substitution text.
pub trait HtsGetIdResolver {
  /// Resolve the id, returning the substituted string if there is a match.
  fn resolve_id(&self, query: &Query) -> Option<String>;
}

/// Determines whether the query matches for use with the resolver.
pub trait QueryMatcher {
  /// Does this query match.
  fn query_matches(&self, query: &Query) -> bool;
}

/// A regex resolver is a resolver that matches ids using Regex.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RegexResolver {
  #[serde(with = "serde_regex")]
  pub regex: Regex,
  pub substitution_string: String,
  pub server: StorageType,
  #[serde(flatten)]
  pub match_guard: MatchOnQuery,
}

/// A query that can be matched with the regex resolver.
#[derive(Clone, Debug, Deserialize)]
pub struct MatchOnQuery {
  pub format: Vec<Format>,
  pub class: Vec<Class>,
  #[serde(with = "serde_regex")]
  pub reference_name: Regex,
  /// The start and end positions are 0-based. [start, end)
  pub start: Interval,
  pub end: Interval,
  pub fields: Fields,
  pub tags: Tags,
  pub no_tags: NoTags,
}

impl Default for MatchOnQuery {
  fn default() -> Self {
    Self {
      format: vec![Bam, Cram, Vcf, Bcf],
      class: vec![Class::Body, Class::Header],
      reference_name: Regex::new(".*").expect("Expected valid regex expression"),
      start: Default::default(),
      end: Default::default(),
      fields: Fields::All,
      tags: Tags::All,
      no_tags: NoTags(None),
    }
  }
}

impl QueryMatcher for Fields {
  fn query_matches(&self, query: &Query) -> bool {
    match (self, &query.fields) {
      (Fields::All, _) => true,
      (Fields::List(self_fields), Fields::List(query_fields)) => self_fields == query_fields,
      (Fields::List(_), Fields::All) => false,
    }
  }
}

impl QueryMatcher for Tags {
  fn query_matches(&self, query: &Query) -> bool {
    match (self, &query.tags) {
      (Tags::All, _) => true,
      (Tags::List(self_tags), Tags::List(query_tags)) => self_tags == query_tags,
      (Tags::List(_), Tags::All) => false,
    }
  }
}

impl QueryMatcher for NoTags {
  fn query_matches(&self, query: &Query) -> bool {
    match (self, &query.no_tags) {
      (NoTags(None), _) => true,
      (NoTags(Some(self_no_tags)), NoTags(Some(query_no_tags))) => self_no_tags == query_no_tags,
      (NoTags(Some(_)), NoTags(None)) => false,
    }
  }
}

impl QueryMatcher for MatchOnQuery {
  fn query_matches(&self, query: &Query) -> bool {
    if let Some(reference_name) = &query.reference_name {
      self.format.contains(&query.format)
        && self.class.contains(&query.class)
        && self.reference_name.is_match(reference_name)
        && self
          .start
          .contains(query.interval.start.unwrap_or(u32::MIN))
        && self.end.contains(query.interval.end.unwrap_or(u32::MAX))
        && self.fields.query_matches(query)
        && self.fields.query_matches(query)
        && self.fields.query_matches(query)
    } else {
      false
    }
  }
}

impl Default for RegexResolver {
  fn default() -> Self {
    Self::new(
      ".*",
      "$0",
      StorageType::default(),
      MatchOnQuery::default(),
    )
    .expect("expected valid resolver")
  }
}

impl RegexResolver {
  /// Create a new regex resolver.
  pub fn new(
    regex: &str,
    replacement_string: &str,
    server: StorageType,
    match_guard: MatchOnQuery,
  ) -> Result<Self, Error> {
    Ok(Self {
      regex: Regex::new(regex)?,
      server,
      substitution_string: replacement_string.to_string(),
      match_guard,
    })
  }
}

impl HtsGetIdResolver for RegexResolver {
  #[instrument(level = "trace", skip(self), ret)]
  fn resolve_id(&self, query: &Query) -> Option<String> {
    if self.regex.is_match(&query.id) && self.match_guard.query_matches(query) {
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
    let resolver = RegexResolver::new(
      ".*",
      "$0-test",
      StorageType::default(),
      MatchOnQuery::default(),
    )
    .unwrap();
    assert_eq!(
      resolver.resolve_id(&Query::new("id", Bam)).unwrap(),
      "id-test"
    );
  }
}
