use std::collections::HashMap;

use htsget_search::htsget::blocking::HtsGet;
use htsget_search::htsget::Response;

use crate::{
  convert_to_query, match_endpoints_get_request, match_endpoints_post_request, merge_responses,
  Endpoint, JsonResponse, PostRequest, Result,
};

pub mod service_info;

/// Gets a JSON response for a GET request. The GET request parameters must
/// be in a HashMap. The "id" field is the only mandatory one. The rest can be
/// consulted [here](https://samtools.github.io/hts-specs/htsget.html)
pub fn get_response_for_get_request(
  searcher: &impl HtsGet,
  mut query_information: HashMap<String, String>,
  endpoint: Endpoint,
) -> Result<JsonResponse> {
  match_endpoints_get_request(&endpoint, &mut query_information)?;
  let query = convert_to_query(&query_information)?;
  searcher
    .search(query)
    .map_err(|error| error.into())
    .map(JsonResponse::from_response)
}

/// Gets a response in JSON for a POST request.
/// The parameters can be consulted [here](https://samtools.github.io/hts-specs/htsget.html)
pub fn get_response_for_post_request(
  searcher: &impl HtsGet,
  mut request: PostRequest,
  id: impl Into<String>,
  endpoint: Endpoint,
) -> Result<JsonResponse> {
  match_endpoints_post_request(&endpoint, &mut request)?;
  let responses = request
    .get_queries(id)?
    .into_iter()
    .map(|query| searcher.search(query).map_err(|error| error.into()))
    .collect::<Result<Vec<Response>>>()?;
  Ok(JsonResponse::from_response(
    // It's okay to unwrap because there will be at least one response
    merge_responses(responses).unwrap(),
  ))
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use htsget_id_resolver::RegexResolver;
  use htsget_search::{
    htsget::blocking::from_storage::HtsGetFromStorage,
    htsget::{Format, Headers, Url},
    storage::blocking::local::LocalStorage,
  };

  use crate::{HtsGetError, Region};

  use super::*;

  #[test]
  fn get_request() {
    let mut request = HashMap::new();
    request.insert("id".to_string(), "bam/htsnexus_test_NA12878".to_string());
    let mut headers = HashMap::new();
    headers.insert("Range".to_string(), "bytes=4668-2596799".to_string());
    assert_eq!(
      get_response_for_get_request(&get_searcher(), request, Endpoint::Reads),
      Ok(JsonResponse::from_response(Response::new(
        Format::Bam,
        vec![Url::new(format!(
          "file://{}",
          get_base_path()
            .join("bam")
            .join("htsnexus_test_NA12878.bam")
            .to_string_lossy()
        ))
        .with_headers(Headers::new(headers))]
      )))
    )
  }

  #[test]
  fn get_reads_request_with_variants_format() {
    let mut request = HashMap::new();
    request.insert("id".to_string(), "bam/htsnexus_test_NA12878".to_string());
    request.insert("format".to_string(), "VCF".to_string());
    assert_eq!(
      get_response_for_get_request(&get_searcher(), request, Endpoint::Reads),
      Err(HtsGetError::UnsupportedFormat(
        "VCF isn't a supported format".to_string()
      ))
    )
  }

  #[test]
  fn get_request_with_range() {
    let mut request = HashMap::new();
    request.insert("id".to_string(), "vcf/sample1-bcbio-cancer".to_string());
    request.insert("referenceName".to_string(), "chrM".to_string());
    request.insert("start".to_string(), "149".to_string());
    request.insert("end".to_string(), "200".to_string());
    let mut headers = HashMap::new();
    headers.insert("Range".to_string(), "bytes=0-3367".to_string());
    assert_eq!(
      get_response_for_get_request(&get_searcher(), request, Endpoint::Variants),
      Ok(JsonResponse::from_response(Response::new(
        Format::Vcf,
        vec![Url::new(format!(
          "file://{}",
          get_base_path()
            .join("vcf")
            .join("sample1-bcbio-cancer.vcf.gz")
            .to_string_lossy()
        ))
        .with_headers(Headers::new(headers))]
      )))
    )
  }

  #[test]
  fn post_request() {
    let request = PostRequest {
      format: None,
      class: None,
      fields: None,
      tags: None,
      notags: None,
      regions: None,
    };
    let mut headers = HashMap::new();
    headers.insert("Range".to_string(), "bytes=4668-2596799".to_string());
    assert_eq!(
      get_response_for_post_request(
        &get_searcher(),
        request,
        "bam/htsnexus_test_NA12878",
        Endpoint::Reads
      ),
      Ok(JsonResponse::from_response(Response::new(
        Format::Bam,
        vec![Url::new(format!(
          "file://{}",
          get_base_path()
            .join("bam")
            .join("htsnexus_test_NA12878.bam")
            .to_string_lossy()
        ))
        .with_headers(Headers::new(headers))]
      )))
    )
  }

  #[test]
  fn post_variants_request_with_reads_format() {
    let request = PostRequest {
      format: Some("BAM".to_string()),
      class: None,
      fields: None,
      tags: None,
      notags: None,
      regions: None,
    };
    assert_eq!(
      get_response_for_post_request(
        &get_searcher(),
        request,
        "bam/htsnexus_test_NA12878",
        Endpoint::Variants
      ),
      Err(HtsGetError::UnsupportedFormat(
        "BAM isn't a supported format".to_string()
      ))
    )
  }

  #[test]
  fn post_request_with_range() {
    let request = PostRequest {
      format: Some("VCF".to_string()),
      class: None,
      fields: None,
      tags: None,
      notags: None,
      regions: Some(vec![Region {
        reference_name: "chrM".to_string(),
        start: Some(149),
        end: Some(200),
      }]),
    };
    let mut headers = HashMap::new();
    headers.insert("Range".to_string(), "bytes=0-3367".to_string());
    assert_eq!(
      get_response_for_post_request(
        &get_searcher(),
        request,
        "vcf/sample1-bcbio-cancer",
        Endpoint::Variants
      ),
      Ok(JsonResponse::from_response(Response::new(
        Format::Vcf,
        vec![Url::new(format!(
          "file://{}",
          get_base_path()
            .join("vcf")
            .join("sample1-bcbio-cancer.vcf.gz")
            .to_string_lossy()
        ))
        .with_headers(Headers::new(headers))]
      )))
    )
  }

  fn get_base_path() -> PathBuf {
    std::env::current_dir()
      .unwrap()
      .parent()
      .unwrap()
      .join("data")
  }

  fn get_searcher() -> impl HtsGet {
    HtsGetFromStorage::new(
      LocalStorage::new("../data", RegexResolver::new(".*", "$0").unwrap()).unwrap(),
    )
  }
}
