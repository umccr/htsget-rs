//! Module providing an implementation of the [HtsGet] trait using a [StorageTrait].
//!

use crate::search::Search;
use crate::{
  bam_search::BamSearch,
  bcf_search::BcfSearch,
  cram_search::CramSearch,
  vcf_search::VcfSearch,
  {HtsGet, Query, Response, Result},
};
use crate::{Format, HtsGetError};
use async_trait::async_trait;
use htsget_config::config::location::Locations;
use htsget_config::resolver::{ResolveResponse, StorageResolver};
use htsget_config::storage;
use htsget_storage::Storage;
use tracing::debug;
use tracing::instrument;

/// Implementation of the [HtsGet] trait using a [StorageTrait].
#[derive(Debug, Clone)]
pub struct HtsGetFromStorage {
  storage: Storage,
}

#[async_trait]
impl HtsGet for Locations {
  async fn search(self, mut query: Query) -> Result<Response> {
    self
      .resolve_request::<HtsGetFromStorage>(&mut query)
      .await
      .ok_or_else(|| HtsGetError::not_found("failed to match query with storage"))?
  }
}

#[async_trait]
impl HtsGet for HtsGetFromStorage {
  #[instrument(level = "debug", skip(self))]
  async fn search(self, query: Query) -> Result<Response> {
    debug!(format = ?query.format(), ?query, "searching {:?}, with query {:?}", query.format(), query);
    match query.format() {
      Format::Bam => BamSearch::new(self.into_inner()).search(query).await,
      Format::Cram => CramSearch::new(self.into_inner()).search(query).await,
      Format::Vcf => VcfSearch::new(self.into_inner()).search(query).await,
      Format::Bcf => BcfSearch::new(self.into_inner()).search(query).await,
    }
  }
}

#[async_trait]
impl ResolveResponse for HtsGetFromStorage {
  async fn from_file(file_storage: &storage::file::File, query: &Query) -> Result<Response> {
    let storage = Storage::from_file(file_storage, query).await?;
    let searcher = HtsGetFromStorage::new(storage);
    searcher.search(query.clone()).await
  }

  #[cfg(feature = "aws")]
  async fn from_s3(s3_storage: &storage::s3::S3, query: &Query) -> Result<Response> {
    let storage = Storage::from_s3(s3_storage, query).await;
    let searcher = HtsGetFromStorage::new(storage?);
    searcher.search(query.clone()).await
  }

  #[cfg(feature = "url")]
  async fn from_url(url_storage_config: &storage::url::Url, query: &Query) -> Result<Response> {
    let storage = Storage::from_url(url_storage_config, query).await;
    let searcher = HtsGetFromStorage::new(storage?);
    searcher.search(query.clone()).await
  }
}

impl HtsGetFromStorage {
  pub fn new(storage: Storage) -> Self {
    Self { storage }
  }

  pub fn storage(&self) -> &Storage {
    &self.storage
  }

  pub fn into_inner(self) -> Storage {
    self.storage
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::fs;
  use std::future::Future;
  use std::path::{Path, PathBuf};
  #[cfg(feature = "aws")]
  use {
    htsget_storage::s3::S3Storage, htsget_test::aws_mocks::with_s3_test_server, std::fs::create_dir,
  };

  use htsget_config::config::location::{Location, LocationEither};
  use htsget_config::storage;
  use htsget_config::storage::Backend;
  use htsget_config::types::Class::Body;
  use htsget_config::types::Scheme::Http;
  use htsget_storage::local::FileStorage;
  #[cfg(feature = "experimental")]
  use htsget_test::c4gh::decrypt_data;
  use htsget_test::http::concat::ConcatResponse;
  use http::uri::Authority;
  use tempfile::TempDir;

  use crate::bam_search::tests::{
    expected_url as bam_expected_url, with_local_storage as with_bam_local_storage, BAM_FILE_NAME,
  };
  use crate::vcf_search::tests::{
    expected_url as vcf_expected_url, with_local_storage as with_vcf_local_storage,
    VCF_FILE_NAME_SPEC,
  };
  use crate::{Headers, Url};

  use super::*;

  #[tokio::test]
  async fn search_bam() {
    with_bam_local_storage(|storage| async move {
      let htsget = HtsGetFromStorage::new(storage);
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam);
      let response = htsget.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(bam_expected_url())
          .with_headers(Headers::default().with_header("Range", "bytes=0-2596798"))],
      ));
      assert_eq!(response, expected_response);

      Some((BAM_FILE_NAME.to_string(), (response.unwrap(), Body).into()))
    })
    .await;
  }

  #[tokio::test]
  async fn search_vcf() {
    with_vcf_local_storage(|storage| async move {
      let htsget = HtsGetFromStorage::new(storage);
      let filename = "spec-v4.3";
      let query = Query::new_with_default_request(filename, Format::Vcf);
      let response = htsget.search(query).await;
      println!("{response:#?}");

      assert_eq!(response, expected_vcf_response(filename));

      Some((
        VCF_FILE_NAME_SPEC.to_string(),
        (response.unwrap(), Body).into(),
      ))
    })
    .await;
  }

  #[tokio::test]
  async fn from_local_storage() {
    with_config_local_storage(
      |_, local_storage| async move {
        let filename = "spec-v4.3";
        let query = Query::new_with_default_request(filename, Format::Vcf);
        let response = HtsGetFromStorage::from_file(&local_storage, &query).await;

        assert_eq!(response, expected_vcf_response(filename));

        Some((
          VCF_FILE_NAME_SPEC.to_string(),
          (response.unwrap(), Body).into(),
        ))
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
        let locations = Locations::new(vec![LocationEither::Simple(Location::new(
          Backend::File(local_storage),
          "".to_string(),
        ))]);

        let filename = "spec-v4.3";
        let query = Query::new_with_default_request(filename, Format::Vcf);
        let response = locations.search(query).await;

        assert_eq!(response, expected_vcf_response(filename));

        Some((
          VCF_FILE_NAME_SPEC.to_string(),
          (response.unwrap(), Body).into(),
        ))
      },
      "data/vcf",
      &[],
    )
    .await;
  }

  fn expected_vcf_response(filename: &str) -> Result<Response> {
    Ok(Response::new(
      Format::Vcf,
      vec![Url::new(vcf_expected_url(filename))
        .with_headers(Headers::default().with_header("Range", "bytes=0-850"))],
    ))
  }

  async fn copy_files_from(from_path: &str, to_path: &Path, copy_files: &[&str]) -> PathBuf {
    let mut base_path = std::env::current_dir()
      .unwrap()
      .parent()
      .unwrap()
      .join(from_path);

    for file_name in copy_files {
      fs::copy(base_path.join(file_name), to_path.join(file_name)).unwrap();
    }
    if !copy_files.is_empty() {
      base_path = PathBuf::from(to_path);
    }

    base_path
  }

  async fn with_config_local_storage_map<M, F, Fut>(
    test: F,
    path: &str,
    copy_files: &[&str],
    map: M,
  ) where
    F: FnOnce(PathBuf, storage::file::File) -> Fut,
    Fut: Future<Output = Option<(String, ConcatResponse)>>,
    M: FnOnce(&[u8]) -> Vec<u8>,
  {
    let tmp_dir = TempDir::new().unwrap();
    let base_path = copy_files_from(path, tmp_dir.path(), copy_files).await;

    println!("{:#?}", base_path);
    let response = test(
      base_path.clone(),
      storage::file::File::new(
        Http,
        Authority::from_static("127.0.0.1:8081"),
        base_path.to_str().unwrap().to_string(),
      ),
    )
    .await;

    read_records(response, &base_path, map).await;
  }

  async fn with_config_local_storage<F, Fut>(test: F, path: &str, copy_files: &[&str])
  where
    F: FnOnce(PathBuf, storage::file::File) -> Fut,
    Fut: Future<Output = Option<(String, ConcatResponse)>>,
  {
    with_config_local_storage_map(test, path, copy_files, |b| b.to_vec()).await;
  }

  async fn read_records<F>(response: Option<(String, ConcatResponse)>, base_path: &Path, map: F)
  where
    F: FnOnce(&[u8]) -> Vec<u8>,
  {
    if let Some((target_file, response)) = response {
      let records = response
        .concat_from_file_path(&base_path.join(target_file))
        .await
        .unwrap();

      let bytes = map(records.merged_bytes());

      records.set_bytes(bytes).read_records().await.unwrap();
    }
  }

  pub(crate) async fn with_local_storage_fn<F, Fut>(test: F, path: &str, copy_files: &[&str])
  where
    F: FnOnce(Storage) -> Fut,
    Fut: Future<Output = Option<(String, ConcatResponse)>>,
  {
    with_config_local_storage(
      |base_path, local_storage| async {
        test(Storage::new(
          FileStorage::new(base_path, local_storage).unwrap(),
        ))
        .await
      },
      path,
      copy_files,
    )
    .await;
  }

  #[cfg(feature = "experimental")]
  pub(crate) async fn with_local_storage_c4gh<F, Fut>(test: F)
  where
    F: FnOnce(Storage) -> Fut,
    Fut: Future<Output = Option<(String, ConcatResponse)>>,
  {
    with_config_local_storage_map(
      |base_path, local_storage| async {
        test(Storage::new(
          FileStorage::new(base_path, local_storage).unwrap(),
        ))
        .await
      },
      "data/c4gh",
      &[],
      decrypt_data,
    )
    .await;
  }

  #[cfg(feature = "aws")]
  pub(crate) async fn with_aws_storage_fn<F, Fut>(test: F, path: &str, copy_files: &[&str])
  where
    F: FnOnce(Storage) -> Fut,
    Fut: Future<Output = Option<(String, ConcatResponse)>>,
  {
    let tmp_dir = TempDir::new().unwrap();
    let to_path = tmp_dir.into_path().join("folder");
    create_dir(&to_path).unwrap();

    let base_path = copy_files_from(path, &to_path, copy_files).await;

    with_aws_s3_storage_fn(
      |storage| async {
        let response = test(storage).await;
        read_records(response, &base_path, |b| b.to_vec()).await;
      },
      "folder".to_string(),
      base_path.parent().unwrap(),
    )
    .await;
  }

  #[cfg(feature = "aws")]
  pub(crate) async fn with_aws_s3_storage_fn<F, Fut>(test: F, folder_name: String, base_path: &Path)
  where
    F: FnOnce(Storage) -> Fut,
    Fut: Future<Output = ()>,
  {
    with_s3_test_server(base_path, |client| async move {
      test(Storage::new(S3Storage::new(client, folder_name))).await;
    })
    .await;
  }
}
