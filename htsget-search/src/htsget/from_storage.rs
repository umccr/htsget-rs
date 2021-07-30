//! Module providing an implementation of the [HtsGet] trait using a [Storage].
//!

use crate::htsget::search::Search;
use crate::{
  htsget::bam_search::BamSearch,
  htsget::bcf_search::BcfSearch,
  htsget::cram_search::CramSearch,
  htsget::vcf_search::VcfSearch,
  htsget::{Format, HtsGet, HtsGetError, Query, Response, Result},
  storage::Storage,
};
use async_trait::async_trait;

/// Implementation of the [HtsGet] trait using a [Storage].
pub struct HtsGetFromStorage<S> {
  storage: S,
}

#[async_trait]
impl<S> HtsGet for HtsGetFromStorage<S>
where
  S: Storage + Sync + Send,
{
  async fn search(&self, query: Query) -> Result<Response> {
    match query.format {
      Some(Format::Bam) | None => BamSearch::new(&self.storage).search(query).await,
      Some(Format::Cram) => CramSearch::new(&self.storage).search(query).await,
      Some(Format::Vcf) => VcfSearch::new(&self.storage).search(query).await,
      Some(Format::Bcf) => BcfSearch::new(&self.storage).search(query).await,
      Some(Format::Unsupported(format)) => Err(HtsGetError::unsupported_format(format)),
    }
  }
}

impl<S> HtsGetFromStorage<S> {
  pub fn new(storage: S) -> Self {
    Self { storage }
  }

  pub fn storage(&self) -> &S {
    &self.storage
  }
}

#[cfg(test)]
mod tests {

  use crate::htsget::bam_search::tests::{
    expected_url as bam_expected_url, with_local_storage as with_bam_local_storage,
  };
  use crate::htsget::vcf_search::tests::{
    expected_url as vcf_expected_url, with_local_storage as with_vcf_local_storage,
  };
  use crate::htsget::{Headers, Url};

  use super::*;

  #[tokio::test]
  async fn search_bam() {
    with_bam_local_storage(|storage| async move {
      let htsget = HtsGetFromStorage::new(storage);
      let query = Query::new("htsnexus_test_NA12878").with_format(Format::Bam);
      let response = htsget.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(bam_expected_url(htsget.storage()))
          .with_headers(Headers::default().with_header("Range", "bytes=4668-2596799"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_vcf() {
    with_vcf_local_storage(|storage| async move {
      let htsget = HtsGetFromStorage::new(storage);
      let filename = "spec-v4.3";
      let query = Query::new(filename).with_format(Format::Vcf);
      let response = htsget.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(vcf_expected_url(htsget.storage(), filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-823"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }
}
