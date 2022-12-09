use http::uri::Authority;
use regex::{Error, Regex};
use serde::{Deserialize, Serialize};
use serde_with::with_prefix;
use tracing::instrument;

use crate::config::{default_localstorage_addr, default_serve_at};
use crate::Format::{Bam, Bcf, Cram, Vcf};
use crate::{Class, Fields, Format, Interval, NoTags, Query, Tags};
use crate::regex_resolver::aws::S3Resolver;
use crate::regex_resolver::Scheme::Https;

#[cfg(feature = "s3-storage")]
pub mod aws;

/// Represents an id resolver, which matches the id, replacing the match in the substitution text.
pub trait IdResolver {
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Scheme {
    #[serde(alias = "http", alias = "HTTP")]
    Http,
    #[serde(alias = "https", alias = "HTTPS")]
    Https
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
    pub scheme: Scheme,
    #[serde(with = "http_serde::authority")]
    pub authority: Authority,
    pub path: String,
}

impl Default for UrlResolver {
    fn default() -> Self {
        Self {
            scheme: Scheme::default(),
            authority: Authority::from_static(default_localstorage_addr()),
            path: default_serve_at().to_string()
        }
    }
}

/// A regex resolver is a resolver that matches ids using Regex.
#[derive(Serialize, Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RegexResolver {
    #[serde(with = "serde_regex")]
    pub regex: Regex,
    pub substitution_string: String,
    pub guard: QueryGuard,
    pub storage_type: StorageType,
}

/// A query that can be matched with the regex resolver.
#[derive(Serialize, Clone, Debug, Deserialize)]
#[serde(default)]
pub struct QueryGuard {
    pub match_formats: Vec<Format>,
    pub match_class: Vec<Class>,
    #[serde(with = "serde_regex")]
    pub match_reference_name: Regex,
    /// The start and end positions are 0-based. [start, end)
    pub start_interval: Interval,
    pub end_interval: Interval,
    pub match_fields: Fields,
    pub match_tags: Tags,
    pub match_no_tags: NoTags,
}

impl Default for QueryGuard {
    fn default() -> Self {
        Self {
            match_formats: vec![Bam, Cram, Vcf, Bcf],
            match_class: vec![Class::Body, Class::Header],
            match_reference_name: Regex::new(".*").expect("Expected valid regex expression"),
            start_interval: Interval { start: Some(0), end: Some(100) },
            end_interval: Default::default(),
            match_fields: Fields::All,
            match_tags: Tags::All,
            match_no_tags: NoTags(None),
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

impl QueryMatcher for QueryGuard {
    fn query_matches(&self, query: &Query) -> bool {
        if let Some(reference_name) = &query.reference_name {
            self.match_formats.contains(&query.format)
                && self.match_class.contains(&query.class)
                && self.match_reference_name.is_match(reference_name)
                && self
                .start_interval
                .contains(query.interval.start.unwrap_or(u32::MIN))
                && self.end_interval.contains(query.interval.end.unwrap_or(u32::MAX))
                && self.match_fields.query_matches(query)
                && self.match_tags.query_matches(query)
                && self.match_no_tags.query_matches(query)
        } else {
            false
        }
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
}

impl IdResolver for RegexResolver {
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
        let resolver = RegexResolver::new(
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
