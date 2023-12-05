pub mod allow_guard;
pub mod object;

use std::result;

use crate::error;
use async_trait::async_trait;
use regex::{Error, Regex};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::config::DataServerConfig;
use crate::resolver::allow_guard::{AllowGuard, QueryAllowed, ReferenceNames};
use crate::resolver::object::ObjectType;
use crate::storage::local::LocalStorage;
#[cfg(feature = "s3-storage")]
use crate::storage::s3::S3Storage;
#[cfg(feature = "url-storage")]
use crate::storage::url::UrlStorageClient;
use crate::storage::{ResolvedId, Storage, TaggedStorageTypes};
use crate::types::{Class, Fields, Format, Interval, Query, Response, Result, Tags};

/// A trait which matches the query id, replacing the match in the substitution text.
pub trait IdResolver {
  /// Resolve the id, returning the substituted string if there is a match.
  fn resolve_id(&self, query: &Query) -> Option<ResolvedId>;
}

/// A trait for determining the response from `Storage`.
#[async_trait]
pub trait ResolveResponse {
  /// Convert from `LocalStorage`.
  async fn from_local(local_storage: &LocalStorage, query: &Query) -> Result<Response>;

  /// Convert from `S3Storage`.
  #[cfg(feature = "s3-storage")]
  async fn from_s3(s3_storage: &S3Storage, query: &Query) -> Result<Response>;

  /// Convert from `UrlStorage`.
  #[cfg(feature = "url-storage")]
  async fn from_url(url_storage: &UrlStorageClient, query: &Query) -> Result<Response>;
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

/// A regex storage is a storage that matches ids using Regex.
#[derive(Serialize, Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Resolver {
  #[serde(with = "serde_regex")]
  regex: Regex,
  // Todo: should match guard be allowed as variables inside the substitution string?
  substitution_string: String,
  storage: Storage,
  allow_guard: AllowGuard,
  object_type: ObjectType,
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

impl IdResolver for Resolver {
  #[instrument(level = "trace", skip(self), ret)]
  fn resolve_id(&self, query: &Query) -> Option<ResolvedId> {
    if self.regex.is_match(query.id()) && self.allow_guard.query_allowed(query) {
      Some(ResolvedId::new(
        self
          .regex
          .replace(query.id(), &self.substitution_string)
          .to_string(),
      ))
    } else {
      None
    }
  }
}

impl Default for Resolver {
  fn default() -> Self {
    Self::new(
      Storage::default(),
      "(data)/(.*)",
      "$2",
      AllowGuard::default(),
      ObjectType::default(),
    )
    .expect("expected valid storage")
  }
}

impl Resolver {
  /// Create a new regex storage.
  pub fn new(
    storage: Storage,
    regex: &str,
    replacement_string: &str,
    allow_guard: AllowGuard,
    object_type: ObjectType,
  ) -> result::Result<Self, Error> {
    Ok(Self {
      regex: Regex::new(regex)?,
      substitution_string: replacement_string.to_string(),
      storage,
      allow_guard,
      object_type,
    })
  }

  /// Validate resolvers and set the local resolvers from the data server config.
  pub fn validate(mut self, config: &DataServerConfig) -> error::Result<Self> {
    if let Storage::Tagged(TaggedStorageTypes::Local) = self.storage() {
      if let Some(local_storage) = config.into() {
        self.storage = Storage::Local { local_storage };
      }
    }

    // `Crypt4GHGenerate` is only supported for `UrlStorage`.
    if let ObjectType::Crypt4GHGenerate = self.object_type() {
      if let Storage::Url { url_storage } = self.storage() {
        if url_storage.endpoints().public_key().is_none() {
          return Err(error::Error::ParseError(
            "the public key endpoint must be set if generating Crypt4GH keys".to_string(),
          ));
        }
      } else {
        return Err(error::Error::ParseError(
          "generating Crypt4GH keys is not supported if not using `UrlStorage`".to_string(),
        ));
      }
    };

    Ok(self)
  }

  /// Get the match associated with the capture group at index `i` using the `regex_match`.
  pub fn get_match<'a>(&'a self, i: usize, regex_match: &'a str) -> Option<&'a str> {
    Some(self.regex().captures(regex_match)?.get(i)?.as_str())
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
    self.allow_guard.allow_interval()
  }

  /// Get allow reference names.
  pub fn allow_reference_names(&self) -> &ReferenceNames {
    self.allow_guard.allow_reference_names()
  }

  /// Get allow fields.
  pub fn allow_fields(&self) -> &Fields {
    self.allow_guard.allow_fields()
  }

  /// Get allow tags.
  pub fn allow_tags(&self) -> &Tags {
    self.allow_guard.allow_tags()
  }

  /// Get the object type config.
  pub fn object_type(&self) -> &ObjectType {
    &self.object_type
  }
}

#[async_trait]
impl StorageResolver for Resolver {
  #[instrument(level = "trace", skip(self), ret)]
  async fn resolve_request<T: ResolveResponse>(
    &self,
    query: &mut Query,
  ) -> Option<Result<Response>> {
    let resolved_id = self.resolve_id(query)?;
    let _matched_id = query.id().to_string();

    query.set_id(resolved_id.into_inner());

    #[cfg(feature = "crypt4gh")]
    if self.object_type().is_crypt4gh() {
      query.set_crypt4gh(true);
    }

    if let Some(response) = self.storage().resolve_local_storage::<T>(query).await {
      return Some(response);
    }

    #[cfg(feature = "s3-storage")]
    {
      let first_match = self.get_match(1, &_matched_id);

      if let Some(response) = self
        .storage()
        .resolve_s3_storage::<T>(first_match, query)
        .await
      {
        return Some(response);
      }
    }

    #[cfg(feature = "url-storage")]
    if let Some(response) = self.storage().resolve_url_storage::<T>(query).await {
      return Some(response);
    }

    None
  }
}

impl IdResolver for &[Resolver] {
  #[instrument(level = "trace", skip(self), ret)]
  fn resolve_id(&self, query: &Query) -> Option<ResolvedId> {
    self.iter().find_map(|resolver| resolver.resolve_id(query))
  }
}

#[async_trait]
impl StorageResolver for &[Resolver] {
  #[instrument(level = "trace", skip(self), ret)]
  async fn resolve_request<T: ResolveResponse>(
    &self,
    query: &mut Query,
  ) -> Option<Result<Response>> {
    for resolver in self.iter() {
      if let Some(resolved_storage) = resolver.resolve_request::<T>(query).await {
        return Some(resolved_storage);
      }
    }

    None
  }
}

#[cfg(test)]
mod tests {
  use http::uri::Authority;

  #[cfg(feature = "url-storage")]
  use {
    crate::storage::url, crate::storage::url::ValidatedUrl, http::Uri as InnerUrl, hyper::Client,
    hyper_rustls::HttpsConnectorBuilder, std::str::FromStr,
  };

  use crate::config::tests::{test_config_from_env, test_config_from_file};
  use crate::storage::url::endpoints::Endpoints;
  use crate::types::Format::Bam;
  use crate::types::Scheme::Http;
  use crate::types::Url;
  #[cfg(feature = "s3-storage")]
  use {crate::storage::s3::S3Storage, std::collections::HashSet};

  use super::*;

  struct TestResolveResponse;

  #[async_trait]
  impl ResolveResponse for TestResolveResponse {
    async fn from_local(local_storage: &LocalStorage, _: &Query) -> Result<Response> {
      Ok(Response::new(
        Bam,
        vec![Url::new(local_storage.authority().to_string())],
      ))
    }

    #[cfg(feature = "s3-storage")]
    async fn from_s3(s3_storage: &S3Storage, _: &Query) -> Result<Response> {
      Ok(Response::new(Bam, vec![Url::new(s3_storage.bucket())]))
    }

    #[cfg(feature = "url-storage")]
    async fn from_url(url_storage: &UrlStorageClient, _: &Query) -> Result<Response> {
      Ok(Response::new(
        Bam,
        vec![Url::new(url_storage.endpoints().file().to_string())],
      ))
    }
  }

  #[tokio::test]
  async fn resolver_resolve_local_request() {
    let local_storage = LocalStorage::new(
      Http,
      Authority::from_static("127.0.0.1:8080"),
      "data".to_string(),
      "/data".to_string(),
    );
    let resolver = Resolver::new(
      Storage::Local { local_storage },
      "id",
      "$0-test",
      AllowGuard::default(),
      ObjectType::default(),
    )
    .unwrap();

    expected_resolved_request(resolver, "127.0.0.1:8080").await;
  }

  #[cfg(feature = "s3-storage")]
  #[tokio::test]
  async fn resolver_resolve_s3_request_tagged() {
    let s3_storage = S3Storage::new("id".to_string(), None, false);
    let resolver = Resolver::new(
      Storage::S3 { s3_storage },
      "(id)-1",
      "$1-test",
      AllowGuard::default(),
      ObjectType::default(),
    )
    .unwrap();

    expected_resolved_request(resolver, "id").await;
  }

  #[cfg(feature = "s3-storage")]
  #[tokio::test]
  async fn resolver_resolve_s3_request() {
    let resolver = Resolver::new(
      Storage::Tagged(TaggedStorageTypes::S3),
      "(id)-1",
      "$1-test",
      AllowGuard::default(),
      ObjectType::default(),
    )
    .unwrap();

    expected_resolved_request(resolver, "id").await;
  }

  #[cfg(feature = "url-storage")]
  #[tokio::test]
  async fn resolver_resolve_url_request() {
    let client = Client::builder().build(
      HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build(),
    );
    let url_storage = UrlStorageClient::new(
      Endpoints::new(
        ValidatedUrl(url::Url {
          inner: InnerUrl::from_str("https://example.com/").unwrap(),
        }),
        ValidatedUrl(url::Url {
          inner: InnerUrl::from_str("https://example.com/").unwrap(),
        }),
        ValidatedUrl(url::Url {
          inner: InnerUrl::from_str("https://example.com/").unwrap(),
        }),
      ),
      ValidatedUrl(url::Url {
        inner: InnerUrl::from_str("https://example.com/").unwrap(),
      }),
      true,
      client,
    );

    let resolver = Resolver::new(
      Storage::Url { url_storage },
      "(id)-1",
      "$1-test",
      AllowGuard::default(),
      ObjectType::default(),
    )
    .unwrap();

    expected_resolved_request(resolver, "https://example.com/").await;
  }

  #[test]
  fn resolver_get_matches() {
    let resolver = Resolver::new(
      Storage::default(),
      "^(id)/(?P<key>.*)$",
      "$0",
      AllowGuard::default(),
      ObjectType::default(),
    )
    .unwrap();
    let first_match = resolver.get_match(1, "id/key").unwrap();

    assert_eq!(first_match, "id");
  }

  #[test]
  fn resolver_get_matches_no_captures() {
    let resolver = Resolver::new(
      Storage::default(),
      "^id/id$",
      "$0",
      AllowGuard::default(),
      ObjectType::default(),
    )
    .unwrap();
    let first_match = resolver.get_match(1, "/id/key");

    assert_eq!(first_match, None);
  }

  #[test]
  fn resolver_resolve_id() {
    let resolver = Resolver::new(
      Storage::default(),
      "id",
      "$0-test",
      AllowGuard::default(),
      ObjectType::default(),
    )
    .unwrap();
    assert_eq!(
      resolver
        .resolve_id(&Query::new_with_default_request("id", Bam))
        .unwrap()
        .into_inner(),
      "id-test"
    );
  }

  #[test]
  fn resolver_array_resolve_id() {
    let resolver = vec![
      Resolver::new(
        Storage::default(),
        "^(id-1)(.*)$",
        "$1-test-1",
        AllowGuard::default(),
        ObjectType::default(),
      )
      .unwrap(),
      Resolver::new(
        Storage::default(),
        "^(id-2)(.*)$",
        "$1-test-2",
        AllowGuard::default(),
        ObjectType::default(),
      )
      .unwrap(),
    ];

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
        let allow_guard = AllowGuard::new(
          ReferenceNames::List(HashSet::from_iter(vec!["chr1".to_string()])),
          Fields::List(HashSet::from_iter(vec!["QNAME".to_string()])),
          Tags::List(HashSet::from_iter(vec!["RG".to_string()])),
          vec![Bam],
          vec![Class::Body],
          Interval::new(Some(100), Some(1000)),
        );
        let resolver = config.resolvers().first().unwrap();
        let expected_storage = S3Storage::new("bucket".to_string(), None, false);

        assert_eq!(resolver.regex().to_string(), "regex");
        assert_eq!(resolver.substitution_string(), "substitution_string");
        assert!(
          matches!(resolver.storage(), Storage::S3 { s3_storage } if s3_storage == &expected_storage)
        );
        assert_eq!(resolver.allow_guard(), &allow_guard);
      },
    );
  }

  async fn expected_resolved_request(resolver: Resolver, expected_id: &str) {
    assert_eq!(
      resolver
        .resolve_request::<TestResolveResponse>(&mut Query::new_with_default_request("id-1", Bam))
        .await
        .unwrap()
        .unwrap(),
      Response::new(Bam, vec![Url::new(expected_id)])
    );
  }
}
