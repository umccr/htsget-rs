//! Resolvers map ids to storage locations.

use crate::config::advanced::allow_guard::QueryAllowed;
use crate::config::advanced::regex_location::RegexLocation;
use crate::config::location::{Location, Locations, PrefixOrId};
use crate::storage;
use crate::storage::{Backend, ResolvedId};
use crate::types::{Query, Response, Result};
use async_trait::async_trait;
use tracing::instrument;

/// A trait which matches the query id, replacing the match in the substitution text.
pub trait IdResolver {
  /// Resolve the id, returning the substituted string if there is a match.
  fn resolve_id(&self, query: &Query) -> Option<ResolvedId>;
}

/// A trait for determining the response from `Storage`.
#[async_trait]
pub trait ResolveResponse {
  /// Convert from `File`.
  async fn from_file(file_storage: &storage::file::File, query: &Query) -> Result<Response>;

  /// Convert from `S3`.
  #[cfg(feature = "aws")]
  async fn from_s3(s3_storage: &storage::s3::S3, query: &Query) -> Result<Response>;

  /// Convert from `Url`.
  #[cfg(feature = "url")]
  async fn from_url(url_storage: &storage::url::Url, query: &Query) -> Result<Response>;
}

/// A trait which uses storage to resolve requests into responses.
#[async_trait]
pub trait StorageResolver {
  /// Resolve a request into a response.
  async fn resolve_request<T: ResolveResponse>(
    &self,
    query: &mut Query,
  ) -> Option<Result<Response>>;
}

/// A type which holds a resolved storage and an resolved id.
#[derive(Debug)]
pub struct ResolvedStorage<T> {
  resolved_storage: T,
  resolved_id: ResolvedId,
}

impl<T> ResolvedStorage<T> {
  /// Create a new resolved storage.
  pub fn new(resolved_storage: T, resolved_id: ResolvedId) -> Self {
    Self {
      resolved_storage,
      resolved_id,
    }
  }

  /// Get the resolved storage.
  pub fn resolved_storage(&self) -> &T {
    &self.resolved_storage
  }

  /// Get the resolved id.
  pub fn resolved_id(&self) -> &ResolvedId {
    &self.resolved_id
  }
}

impl IdResolver for Location {
  #[instrument(level = "trace", skip(self), ret)]
  fn resolve_id(&self, query: &Query) -> Option<ResolvedId> {
    let replace = |regex_location: &RegexLocation| {
      Some(
        regex_location
          .regex()
          .replace(query.id(), regex_location.substitution_string())
          .to_string(),
      )
    };

    let resolved_id = match self {
      Location::Simple(location) => match location.prefix_or_id().unwrap_or_default() {
        PrefixOrId::Prefix(prefix) if query.id().starts_with(&prefix) => {
          Some(format!("{}/{}", location.to_append(), query.id()))
        }
        PrefixOrId::Id(id) => {
          if query.id() == id.as_str() {
            Some(location.to_append().to_string())
          } else {
            None
          }
        }
        _ => None,
      },
      Location::Regex(regex_location) => {
        if regex_location.regex().is_match(query.id()) {
          if let Some(guard) = regex_location.guard() {
            if guard.query_allowed(query) {
              replace(regex_location)
            } else {
              None
            }
          } else {
            replace(regex_location)
          }
        } else {
          None
        }
      }
    };

    resolved_id.map(|id| {
      let id = id.strip_prefix("/").unwrap_or(&id);
      ResolvedId::new(id.to_string())
    })
  }
}

#[async_trait]
impl StorageResolver for Location {
  #[instrument(level = "trace", skip(self), ret)]
  async fn resolve_request<T: ResolveResponse>(
    &self,
    query: &mut Query,
  ) -> Option<Result<Response>> {
    let resolved_id = self.resolve_id(query)?;
    let _matched_id = query.id().to_string();

    query.set_id(resolved_id.into_inner());

    match self.backend() {
      Backend::File(file) => Some(T::from_file(file, query).await),
      #[cfg(feature = "aws")]
      Backend::S3(s3) => {
        let s3 = if let Self::Regex(regex_location) = self {
          if s3.bucket().is_empty() {
            let first_match = regex_location
              .regex()
              .captures(&_matched_id)?
              .get(1)?
              .as_str()
              .to_string();
            &s3.clone().with_bucket(first_match)
          } else {
            s3
          }
        } else {
          s3
        };

        Some(T::from_s3(s3, query).await)
      }
      #[cfg(feature = "url")]
      Backend::Url(url_storage) => Some(T::from_url(url_storage, query).await),
    }
  }
}

impl IdResolver for &[Location] {
  #[instrument(level = "trace", skip(self), ret)]
  fn resolve_id(&self, query: &Query) -> Option<ResolvedId> {
    self.iter().find_map(|location| location.resolve_id(query))
  }
}

#[async_trait]
impl StorageResolver for &[Location] {
  #[instrument(level = "trace", skip(self), ret)]
  async fn resolve_request<T: ResolveResponse>(
    &self,
    query: &mut Query,
  ) -> Option<Result<Response>> {
    for location in self.iter() {
      if let Some(location) = location.resolve_request::<T>(query).await {
        return Some(location);
      }
    }

    None
  }
}

impl IdResolver for Locations {
  #[instrument(level = "trace", skip(self), ret)]
  fn resolve_id(&self, query: &Query) -> Option<ResolvedId> {
    self.as_slice().resolve_id(query)
  }
}

#[async_trait]
impl StorageResolver for Locations {
  #[instrument(level = "trace", skip(self), ret)]
  async fn resolve_request<T: ResolveResponse>(
    &self,
    query: &mut Query,
  ) -> Option<Result<Response>> {
    self.as_slice().resolve_request::<T>(query).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::location::SimpleLocation;
  use crate::config::tests::{test_config_from_env, test_config_from_file};
  use crate::storage;
  use crate::types::Format::Bam;
  use crate::types::Scheme::Http;
  use crate::types::Url;
  use http::uri::Authority;
  #[cfg(feature = "url")]
  use reqwest::ClientBuilder;
  #[cfg(feature = "aws")]
  use {
    crate::config::advanced::allow_guard::{AllowGuard, ReferenceNames},
    crate::types::{Class, Fields, Interval, Tags},
    std::collections::HashSet,
  };

  struct TestResolveResponse;

  #[async_trait]
  impl ResolveResponse for TestResolveResponse {
    async fn from_file(file: &storage::file::File, query: &Query) -> Result<Response> {
      Ok(Response::new(
        Bam,
        Self::format_url(file.authority().as_ref(), query.id()),
      ))
    }

    #[cfg(feature = "aws")]
    async fn from_s3(s3_storage: &storage::s3::S3, query: &Query) -> Result<Response> {
      Ok(Response::new(
        Bam,
        Self::format_url(s3_storage.bucket(), query.id()),
      ))
    }

    #[cfg(feature = "url")]
    async fn from_url(url: &storage::url::Url, query: &Query) -> Result<Response> {
      Ok(Response::new(
        Bam,
        Self::format_url(url.url().to_string().strip_suffix('/').unwrap(), query.id()),
      ))
    }
  }

  impl TestResolveResponse {
    fn format_url(prefix: &str, id: &str) -> Vec<Url> {
      vec![Url::new(format!("{prefix}/{id}"))]
    }
  }

  #[tokio::test]
  async fn resolver_resolve_local_request() {
    let file = storage::file::File::new(
      Http,
      Authority::from_static("127.0.0.1:8080"),
      "data".to_string(),
    );

    let regex_location = RegexLocation::new(
      "id".parse().unwrap(),
      "$0-test".to_string(),
      Backend::File(file.clone()),
      Default::default(),
    );
    expected_resolved_request(vec![regex_location.into()], "127.0.0.1:8080/id-test-1").await;

    let location = SimpleLocation::new(
      Backend::File(file),
      "".to_string(),
      Some(PrefixOrId::Prefix("".to_string())),
    );
    expected_resolved_request(vec![location.into()], "127.0.0.1:8080/id-1").await;
  }

  #[cfg(feature = "aws")]
  #[tokio::test]
  async fn resolver_resolve_s3_request_tagged() {
    let s3_storage = storage::s3::S3::new("id2".to_string(), None, false);
    let regex_location = RegexLocation::new(
      "(id)-1".parse().unwrap(),
      "$1-test".to_string(),
      Backend::S3(s3_storage.clone()),
      Default::default(),
    );
    expected_resolved_request(vec![regex_location.into()], "id2/id-test").await;

    let location = SimpleLocation::new(
      Backend::S3(s3_storage),
      "".to_string(),
      Some(PrefixOrId::Prefix("".to_string())),
    );
    expected_resolved_request(vec![location.into()], "id2/id-1").await;
  }

  #[cfg(feature = "aws")]
  #[tokio::test]
  async fn resolver_resolve_s3_request() {
    let regex_location = RegexLocation::new(
      "(id)-1".parse().unwrap(),
      "$1-test".to_string(),
      Backend::S3(storage::s3::S3::default()),
      Default::default(),
    );
    expected_resolved_request(vec![regex_location.clone().into()], "id/id-test").await;

    let regex_location = RegexLocation::new(
      "^(id)-(?P<key>.*)$".parse().unwrap(),
      "$key".to_string(),
      Backend::S3(storage::s3::S3::default()),
      Default::default(),
    );
    expected_resolved_request(vec![regex_location.clone().into()], "id/1").await;

    let location = SimpleLocation::new(
      Backend::S3(storage::s3::S3::new("bucket".to_string(), None, false)),
      "".to_string(),
      Some(PrefixOrId::Prefix("".to_string())),
    );
    expected_resolved_request(vec![location.into()], "bucket/id-1").await;
  }

  #[cfg(feature = "url")]
  #[tokio::test]
  async fn resolver_resolve_url_request() {
    let client =
      reqwest_middleware::ClientBuilder::new(ClientBuilder::new().build().unwrap()).build();
    let url_storage = storage::url::Url::new(
      "https://example.com/".parse().unwrap(),
      "https://example.com/".parse().unwrap(),
      true,
      vec![],
      client,
    );

    let regex_location = RegexLocation::new(
      "(id)-1".parse().unwrap(),
      "$1-test".to_string(),
      Backend::Url(url_storage.clone()),
      Default::default(),
    );
    expected_resolved_request(
      vec![regex_location.clone().into()],
      "https://example.com/id-test",
    )
    .await;

    let location = SimpleLocation::new(
      Backend::Url(url_storage),
      "".to_string(),
      Some(PrefixOrId::Prefix("".to_string())),
    );
    expected_resolved_request(vec![location.into()], "https://example.com/id-1").await;
  }

  #[test]
  fn resolver_array_resolve_id() {
    let resolver = Locations::new(vec![
      RegexLocation::new(
        "^(id-1)(.*)$".parse().unwrap(),
        "$1-test-1".to_string(),
        Default::default(),
        Default::default(),
      )
      .into(),
      RegexLocation::new(
        "^(id-2)(.*)$".parse().unwrap(),
        "$1-test-2".to_string(),
        Default::default(),
        Default::default(),
      )
      .into(),
    ]);

    assert_eq!(
      resolver
        .as_slice()
        .resolve_id(&Query::new_with_default_request("id-1", Bam))
        .unwrap()
        .into_inner(),
      "id-1-test-1"
    );
    assert_eq!(
      resolver
        .as_slice()
        .resolve_id(&Query::new_with_default_request("id-2", Bam))
        .unwrap()
        .into_inner(),
      "id-2-test-2"
    );

    let resolver = Locations::new(vec![
      SimpleLocation::new(
        Default::default(),
        "".to_string(),
        Some(PrefixOrId::Prefix("id-1".to_string())),
      )
      .into(),
      SimpleLocation::new(
        Default::default(),
        "".to_string(),
        Some(PrefixOrId::Prefix("id-2".to_string())),
      )
      .into(),
    ]);
    assert_eq!(
      resolver
        .as_slice()
        .resolve_id(&Query::new_with_default_request("id-1", Bam))
        .unwrap()
        .into_inner(),
      "id-1"
    );
    assert_eq!(
      resolver
        .as_slice()
        .resolve_id(&Query::new_with_default_request("id-2", Bam))
        .unwrap()
        .into_inner(),
      "id-2"
    );
    let resolver = Locations::new(vec![
      SimpleLocation::new(
        Default::default(),
        "append_to".to_string(),
        Some(PrefixOrId::Prefix("id-1".to_string())),
      )
      .into(),
      SimpleLocation::new(
        Default::default(),
        "append_to".to_string(),
        Some(PrefixOrId::Prefix("id-2".to_string())),
      )
      .into(),
    ]);
    assert_eq!(
      resolver
        .as_slice()
        .resolve_id(&Query::new_with_default_request("id-1", Bam))
        .unwrap()
        .into_inner(),
      "append_to/id-1"
    );
    assert_eq!(
      resolver
        .as_slice()
        .resolve_id(&Query::new_with_default_request("id-2", Bam))
        .unwrap()
        .into_inner(),
      "append_to/id-2"
    );

    let resolver = Locations::new(vec![
      SimpleLocation::new(
        Default::default(),
        "append_to".to_string(),
        Some(PrefixOrId::Id("id-1".to_string())),
      )
      .into(),
      SimpleLocation::new(
        Default::default(),
        "append_to".to_string(),
        Some(PrefixOrId::Id("id-2".to_string())),
      )
      .into(),
    ]);
    assert_eq!(
      resolver
        .as_slice()
        .resolve_id(&Query::new_with_default_request("id-1", Bam))
        .unwrap()
        .into_inner(),
      "append_to"
    );
    assert_eq!(
      resolver
        .as_slice()
        .resolve_id(&Query::new_with_default_request("id-2", Bam))
        .unwrap()
        .into_inner(),
      "append_to"
    );
  }

  #[test]
  fn config_resolvers_file() {
    test_config_from_file(
      r#"
        [[locations]]
        regex = "regex"
        "#,
      |config| {
        let regex = config.locations().first().unwrap().as_regex().unwrap();
        assert_eq!(regex.regex().as_str(), "regex");
      },
    );
  }

  #[test]
  fn config_resolvers_guard_file() {
    test_config_from_file(
      r#"
      [[locations]]
      regex = "regex"

      [locations.guard]
      allow_formats = ["BAM"]
      "#,
      |config| {
        let regex = config.locations().first().unwrap().as_regex().unwrap();
        assert_eq!(regex.guard().unwrap().allow_formats(), &vec![Bam]);
      },
    );
  }

  #[test]
  fn config_resolvers_env() {
    test_config_from_env(vec![("HTSGET_LOCATIONS", "[{regex=regex}]")], |config| {
      let regex = config.locations().first().unwrap().as_regex().unwrap();
      assert_eq!(regex.regex().as_str(), "regex");
    });
  }

  #[cfg(feature = "aws")]
  #[test]
  fn config_resolvers_all_options_env() {
    test_config_from_env(
      vec![(
        "HTSGET_LOCATIONS",
        "[{ regex=regex, substitution_string=substitution_string, \
        backend={ kind=S3, bucket=bucket }, \
        guard={ allow_reference_names=[chr1], allow_fields=[QNAME], allow_tags=[RG], \
        allow_formats=[BAM], allow_classes=[body], allow_interval={ start=100, \
        end=1000 } } }]",
      )],
      |config| {
        let allow_guard = AllowGuard::new(
          ReferenceNames::List(HashSet::from_iter(vec!["chr1".to_string()])),
          Fields::List(HashSet::from_iter(vec!["QNAME".to_string()])),
          Tags::List(HashSet::from_iter(vec!["RG".to_string()])),
          vec![Bam],
          vec![Class::Body],
          Interval::new(Some(100), Some(1000)),
        );
        let resolver = config.locations().first().unwrap();
        let expected_storage = storage::s3::S3::new("bucket".to_string(), None, false);
        let Backend::S3(storage) = resolver.backend() else {
          panic!();
        };

        assert_eq!(storage.bucket(), expected_storage.bucket());
        assert_eq!(storage.endpoint(), expected_storage.endpoint());
        assert_eq!(storage.path_style(), expected_storage.path_style());

        let regex = config.locations().first().unwrap().as_regex().unwrap();
        assert_eq!(regex.regex().to_string(), "regex");
        assert_eq!(regex.substitution_string(), "substitution_string");
        assert_eq!(regex.guard().unwrap(), &allow_guard);
      },
    );
  }

  async fn expected_resolved_request(resolver: Vec<Location>, expected_id: &str) {
    assert_eq!(
      Locations::new(resolver)
        .resolve_request::<TestResolveResponse>(&mut Query::new_with_default_request("id-1", Bam))
        .await
        .unwrap()
        .unwrap(),
      Response::new(Bam, vec![Url::new(expected_id)])
    );
  }
}
