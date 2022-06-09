//! Module providing an implementation of the [HtsGet] trait using a [Storage].
//!

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncSeek};
use tracing::debug;

use htsget_config::regex_resolver::RegexResolver;

use crate::htsget::search::Search;
use crate::htsget::Format;
#[cfg(feature = "s3-storage")]
use crate::storage::aws::AwsS3Storage;
#[cfg(feature = "gds-storage")]
use crate::storage::gds::GDSStorage;
use crate::storage::local::LocalStorage;
use crate::storage::UrlFormatter;
use crate::{
  htsget::bam_search::BamSearch,
  htsget::bcf_search::BcfSearch,
  htsget::cram_search::CramSearch,
  htsget::vcf_search::VcfSearch,
  htsget::{HtsGet, Query, Response, Result},
  storage::Storage,
};

/// Implementation of the [HtsGet] trait using a [Storage].
#[derive(Debug, Clone)]
pub struct HtsGetFromStorage<S> {
  storage_ref: Arc<S>,
}

#[async_trait]
impl<S, R> HtsGet for HtsGetFromStorage<S>
where
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin,
  S: Storage<Streamable = R> + Sync + Send + 'static,
{
  async fn search(&self, query: Query) -> Result<Response> {
    debug!(?query.format, ?query, "Searching {:?}, with query {:?}.", query.format, query);
    let response = match query.format {
      Format::Bam => BamSearch::new(self.storage()).search(query).await,
      Format::Cram => CramSearch::new(self.storage()).search(query).await,
      Format::Vcf => VcfSearch::new(self.storage()).search(query).await,
      Format::Bcf => BcfSearch::new(self.storage()).search(query).await,
    };
    debug!(?response, "Response obtained {:?}", response);
    response
  }

  fn get_supported_formats(&self) -> Vec<Format> {
    vec![Format::Bam, Format::Cram, Format::Vcf, Format::Bcf]
  }

  fn are_field_parameters_effective(&self) -> bool {
    false
  }

  fn are_tag_parameters_effective(&self) -> bool {
    false
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

#[cfg(feature = "s3-storage")]
impl HtsGetFromStorage<AwsS3Storage> {
  pub async fn s3_from(bucket: String, resolver: RegexResolver) -> Self {
    HtsGetFromStorage::new(AwsS3Storage::new_with_default_config(bucket, resolver).await)
  }
}

#[cfg(feature = "gds-storage")]
impl HtsGetFromStorage<GDSStorage> {
  pub async fn gds_from(resolver: RegexResolver) -> Self {
    HtsGetFromStorage::new(GDSStorage::new(resolver).await)
  }
}

impl<T: UrlFormatter + Send + Sync> HtsGetFromStorage<LocalStorage<T>> {
  pub fn local_from<P: AsRef<Path>>(
    path: P,
    resolver: RegexResolver,
    formatter: T,
  ) -> Result<Self> {
    Ok(HtsGetFromStorage::new(LocalStorage::new(
      path, resolver, formatter,
    )?))
  }
}

#[cfg(test)]
mod tests {
  use htsget_test_utils::util::expected_bgzf_eof_data_url;

  use crate::htsget::bam_search::tests::{
    expected_url as bam_expected_url, with_local_storage as with_bam_local_storage,
  };
  use crate::htsget::vcf_search::tests::{
    expected_url as vcf_expected_url, with_local_storage as with_vcf_local_storage,
  };
  use crate::htsget::{Class::Body, Headers, Url};

  use super::*;

  #[tokio::test]
  async fn search_bam() {
    with_bam_local_storage(|storage| async move {
      let htsget = HtsGetFromStorage::new(Arc::try_unwrap(storage).unwrap());
      let query = Query::new("htsnexus_test_NA12878", Format::Bam);
      let response = htsget.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(bam_expected_url())
            .await
            .with_headers(Headers::default().with_header("Range", "bytes=0-2596770")),
          Url::new(expected_bgzf_eof_data_url())
            .await
            .with_class(Body),
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
      let query = Query::new(filename, Format::Vcf);
      let response = htsget.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![
          Url::new(vcf_expected_url(filename))
            .await
            .with_headers(Headers::default().with_header("Range", "bytes=0-822")),
          Url::new(expected_bgzf_eof_data_url())
            .await
            .with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }
}
