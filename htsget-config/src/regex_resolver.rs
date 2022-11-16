use crate::Format::{Bam, Bcf, Cram, Vcf};
use crate::{Class, Fields, Format, Interval, NoTags, Query, Tags};
use regex::{Error, Regex};
use serde::Deserialize;
use tracing::instrument;

/// Represents an id resolver, which matches the id, replacing the match in the substitution text.
pub trait HtsGetIdResolver {
  /// Resolve the id, returning the substituted string if there is a match.
  fn resolve_id(&self, query: &Query) -> Option<String>;
}

/// A regex resolver is a resolver that matches ids using Regex.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RegexResolver {
  #[serde(with = "serde_regex")]
  pub regex: Regex,
  pub substitution_string: String,
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

impl Default for RegexResolver {
  fn default() -> Self {
    Self::new(".*", "$0", MatchOnQuery::default()).expect("expected valid resolver")
  }
}

impl RegexResolver {
  /// Create a new regex resolver.
  pub fn new(
    regex: &str,
    replacement_string: &str,
    match_guard: MatchOnQuery,
  ) -> Result<Self, Error> {
    Ok(Self {
      regex: Regex::new(regex)?,
      substitution_string: replacement_string.to_string(),
      match_guard,
    })
  }
}

impl HtsGetIdResolver for RegexResolver {
  #[instrument(level = "trace", skip(self), ret)]
  fn resolve_id(&self, query: &Query) -> Option<String> {
    if self.regex.is_match(&query.id) {
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
    let resolver = RegexResolver::new(".*", "$0-test", MatchOnQuery::default()).unwrap();
    assert_eq!(
      resolver.resolve_id(&Query::new("id", Bam)).unwrap(),
      "id-test"
    );
  }
}
