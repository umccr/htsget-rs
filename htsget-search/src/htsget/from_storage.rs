//! Module providing an implementation of the [HtsGet] trait using a [Storage].
//!

use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::AsyncRead;
use tracing::debug;
use tracing::instrument;

use htsget_config::resolver::{ResolveResponse, StorageResolver};
use htsget_config::storage::local::LocalStorage as LocalStorageConfig;
#[cfg(feature = "s3-storage")]
use {crate::storage::s3::S3Storage, htsget_config::storage::s3::S3Storage as S3StorageConfig};
#[cfg(feature = "url-storage")]
use {
  crate::storage::url::UrlStorage,
  htsget_config::storage::url::UrlStorageClient as UrlStorageConfig,
};

use crate::htsget::search::Search;
use crate::storage::local::LocalStorage;
use crate::Resolver;
use crate::{
  htsget::bam_search::BamSearch,
  htsget::bcf_search::BcfSearch,
  htsget::cram_search::CramSearch,
  htsget::vcf_search::VcfSearch,
  htsget::{HtsGet, Query, Response, Result},
  storage::Storage,
};
use crate::{Format, HtsGetError};

/// Implementation of the [HtsGet] trait using a [Storage].
#[derive(Debug, Clone)]
pub struct HtsGetFromStorage<S> {
  storage_ref: Arc<S>,
}

#[async_trait]
impl HtsGet for Vec<Resolver> {
  async fn search(&self, query: Query) -> Result<Response> {
    self.as_slice().search(query).await
  }
}

#[async_trait]
impl HtsGet for &[Resolver] {
  async fn search(&self, mut query: Query) -> Result<Response> {
    self
      .resolve_request::<HtsGetFromStorage<()>>(&mut query)
      .await
      .ok_or_else(|| HtsGetError::not_found("failed to match query with storage"))?
  }
}

#[async_trait]
impl<S, R> HtsGet for HtsGetFromStorage<S>
where
  R: AsyncRead + Send + Sync + Unpin + 'static,
  S: Storage<Streamable = R> + Sync + Send + 'static,
{
  #[instrument(level = "debug", skip(self))]
  async fn search(&self, query: Query) -> Result<Response> {
    debug!(format = ?query.format(), ?query, "searching {:?}, with query {:?}", query.format(), query);
    match query.format() {
      Format::Bam => BamSearch::new(self.storage()).search(query).await,
      Format::Cram => CramSearch::new(self.storage()).search(query).await,
      Format::Vcf => VcfSearch::new(self.storage()).search(query).await,
      Format::Bcf => BcfSearch::new(self.storage()).search(query).await,
    }
  }
}

#[async_trait]
impl<S> ResolveResponse for HtsGetFromStorage<S> {
  async fn from_local(
    local_storage_config: &LocalStorageConfig,
    query: &Query,
  ) -> Result<Response> {
    let local_storage = local_storage_config.clone();
    let path = local_storage.local_path().to_string();
    let searcher = HtsGetFromStorage::new(LocalStorage::new(path, local_storage)?);
    searcher.search(query.clone()).await
  }

  #[cfg(feature = "s3-storage")]
  async fn from_s3(s3_storage: &S3StorageConfig, query: &Query) -> Result<Response> {
    let searcher = HtsGetFromStorage::new(
      S3Storage::new_with_default_config(
        s3_storage.bucket().to_string(),
        s3_storage.clone().endpoint(),
        s3_storage.clone().path_style(),
      )
      .await,
    );
    searcher.search(query.clone()).await
  }

  #[cfg(feature = "url-storage")]
  async fn from_url(url_storage_config: &UrlStorageConfig, query: &Query) -> Result<Response> {
    let searcher = HtsGetFromStorage::new(UrlStorage::new(
      url_storage_config.client_cloned(),
      url_storage_config.endpoint_head().clone(),
      url_storage_config.endpoint_file().clone(),
      url_storage_config.endpoint_index().clone(),
      url_storage_config.response_scheme(),
      url_storage_config.forward_headers(),
      #[cfg(feature = "crypt4gh")]
      url_storage_config.endpoint_crypt4gh_header().cloned(),
    ));
    searcher.search(query.clone()).await
  }
}

impl<S> HtsGetFromStorage<S> {
  pub fn new(storage: S) -> Self {
    Self {
      storage_ref: Arc::new(storage),
    }
  }

  pub fn storage(&self) -> Arc<S> {
    Arc::clone(&self.storage_ref)
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::fs;
  #[cfg(feature = "s3-storage")]
  use std::fs::create_dir;
  use std::future::Future;
  use std::path::{Path, PathBuf};

  use http::uri::Authority;
  use tempfile::TempDir;

  use htsget_config::storage;
  use htsget_config::types::Scheme::Http;
  use htsget_test::util::expected_bgzf_eof_data_url;

  use crate::htsget::bam_search::tests::{
    expected_url as bam_expected_url, with_local_storage as with_bam_local_storage,
  };
  use crate::htsget::vcf_search::tests::{
    expected_url as vcf_expected_url, with_local_storage as with_vcf_local_storage,
  };
  #[cfg(feature = "s3-storage")]
  use crate::storage::s3::tests::with_aws_s3_storage_fn;
  use crate::{Headers, Url};

  use super::*;

  #[tokio::test]
  async fn search_bam() {
    with_bam_local_storage(|storage| async move {
      let htsget = HtsGetFromStorage::new(Arc::try_unwrap(storage).unwrap());
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam);
      let response = htsget.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(bam_expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-2596770")),
          Url::new(expected_bgzf_eof_data_url()),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_vcf() {
    with_vcf_local_storage(|storage| async move {
      let htsget = HtsGetFromStorage::new(Arc::try_unwrap(storage).unwrap());
      let filename = "spec-v4.3";
      let query = Query::new_with_default_request(filename, Format::Vcf);
      let response = htsget.search(query).await;
      println!("{response:#?}");

      assert_eq!(response, expected_vcf_response(filename));
    })
    .await;
  }

  #[tokio::test]
  async fn from_local_storage() {
    with_config_local_storage(
      |_, local_storage| async move {
        let filename = "spec-v4.3";
        let query = Query::new_with_default_request(filename, Format::Vcf);
        let response = HtsGetFromStorage::<()>::from_local(&local_storage, &query).await;

        assert_eq!(response, expected_vcf_response(filename));
      },
      "data/vcf",
      &[],
    )
    .await;
  }

  #[tokio::test]
  async fn search_resolvers() {
    with_config_local_storage(
      |_, local_storage| async {
        let resolvers = vec![Resolver::new(
          storage::Storage::Local { local_storage },
          ".*",
          "$0",
          Default::default(),
          Default::default(),
        )
        .unwrap()];

        let filename = "spec-v4.3";
        let query = Query::new_with_default_request(filename, Format::Vcf);
        let response = resolvers.search(query).await;

        assert_eq!(response, expected_vcf_response(filename));
      },
      "data/vcf",
      &[],
    )
    .await;
  }

  fn expected_vcf_response(filename: &str) -> Result<Response> {
    Ok(Response::new(
      Format::Vcf,
      vec![
        Url::new(vcf_expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-822")),
        Url::new(expected_bgzf_eof_data_url()),
      ],
    ))
  }

  async fn copy_files(from_path: &str, to_path: &Path, file_names: &[&str]) -> PathBuf {
    let mut base_path = std::env::current_dir()
      .unwrap()
      .parent()
      .unwrap()
      .join(from_path);

    for file_name in file_names {
      fs::copy(base_path.join(file_name), to_path.join(file_name)).unwrap();
    }
    if !file_names.is_empty() {
      base_path = PathBuf::from(to_path);
    }

    base_path
  }

  async fn with_config_local_storage<F, Fut>(test: F, path: &str, file_names: &[&str])
  where
    F: FnOnce(PathBuf, LocalStorageConfig) -> Fut,
    Fut: Future<Output = ()>,
  {
    let tmp_dir = TempDir::new().unwrap();
    let base_path = copy_files(path, tmp_dir.path(), file_names).await;

    println!("{:#?}", base_path);
    test(
      base_path.clone(),
      LocalStorageConfig::new(
        Http,
        Authority::from_static("127.0.0.1:8081"),
        base_path.to_str().unwrap().to_string(),
        "/data".to_string(),
      ),
    )
    .await
  }

  pub(crate) async fn with_local_storage_fn<F, Fut>(test: F, path: &str, file_names: &[&str])
  where
    F: FnOnce(Arc<LocalStorage<LocalStorageConfig>>) -> Fut,
    Fut: Future<Output = ()>,
  {
    with_config_local_storage(
      |base_path, local_storage| async {
        test(Arc::new(
          LocalStorage::new(base_path, local_storage).unwrap(),
        ))
        .await
      },
      path,
      file_names,
    )
    .await;
  }

  #[cfg(feature = "s3-storage")]
  pub(crate) async fn with_aws_storage_fn<F, Fut>(test: F, path: &str, file_names: &[&str])
  where
    F: FnOnce(Arc<S3Storage>) -> Fut,
    Fut: Future<Output = ()>,
  {
    let tmp_dir = TempDir::new().unwrap();
    let to_path = tmp_dir.into_path().join("folder");
    create_dir(&to_path).unwrap();

    let base_path = copy_files(path, &to_path, file_names).await;

    with_aws_s3_storage_fn(test, "folder".to_string(), base_path.parent().unwrap()).await;
  }
}
