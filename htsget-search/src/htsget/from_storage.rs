//! Module providing an implementation of the [HtsGet] trait using a [Storage].
//!

use crate::{
  htsget::bam_search::BamSearch,
  htsget::vcf_search::VCFSearch,
  htsget::{Format, HtsGet, HtsGetError, Query, Response, Result},
  storage::Storage,
};

/// Implementation of the [HtsGet] trait using a [Storage].
pub struct HtsGetFromStorage<S> {
  storage: S,
}

impl<S> HtsGet for HtsGetFromStorage<S>
where
  S: Storage,
{
  fn search(&self, query: Query) -> Result<Response> {
    match query.format {
      Some(Format::Bam) | None => BamSearch::new(&self.storage).search(query),
      Some(Format::Vcf) => VCFSearch::new(&self.storage).search(query),
      Some(format) => Err(HtsGetError::unsupported_format(format)),
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
    expected_url as bam_expected_url, with_local_storage as bam_with_local_storage,
  };
  use crate::htsget::vcf_search::tests::{
    expected_url as vcf_expected_url, with_local_storage as vcf_with_local_storage,
  };
  use crate::htsget::{Headers, Url};

  use super::*;

  #[test]
  fn search_bam() {
    bam_with_local_storage(|storage| {
      let htsget = HtsGetFromStorage::new(storage);
      let query = Query::new("htsnexus_test_NA12878").with_format(Format::Bam);
      let response = htsget.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(bam_expected_url(&htsget.storage()))
          .with_headers(Headers::default().with_header("Range", "bytes=4668-"))],
      ));
      assert_eq!(response, expected_response)
    })
  }

  #[test]
  fn search_vcf() {
    vcf_with_local_storage(|storage| {
      let htsget = HtsGetFromStorage::new(storage);
      let filename = "spec-v4.3";
      let query = Query::new(filename).with_format(Format::Vcf);
      let response = htsget.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(vcf_expected_url(&htsget.storage(), filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-851"))],
      ));
      assert_eq!(response, expected_response)
    })
  }
}
