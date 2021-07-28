mod query_builder;
use query_builder::QueryBuilder;
mod error;
pub use error::{HtsGetError, Result};
mod json_response;
use json_response::JsonResponse;
mod post_request;
pub use post_request::{PostRequest, Region};

use htsget_search::htsget::{HtsGet, Query, Response};
use std::collections::HashMap;

const READS_DEFAULT_FORMAT: &str = "BAM";
const VARIANTS_DEFAULT_FORMAT: &str = "VCF";
const READS_FORMATS: [&str; 2] = ["BAM", "CRAM"];
const VARIANTS_FORMATS: [&str; 2] = ["VCF", "BCF"];

/// A enum to distinguish between the two endpoint defined in the
/// [HtsGet specification](https://samtools.github.io/hts-specs/htsget.html)
pub enum Endpoint {
  Reads,
  Variants,
}

/// Gets a JSON response for a GET request. The GET request parameters must
/// be in a HashMap. The "id" field is the only mandatory one. The rest can be
/// consulted [here](https://samtools.github.io/hts-specs/htsget.html)
pub fn get_response_for_get_request<H: HtsGet>(
  searcher: &H,
  mut query_information: HashMap<String, String>,
  endpoint: Endpoint,
) -> Result<String> {
  match (endpoint, query_information.get(&"format".to_string())) {
    (Endpoint::Reads, None) => {
      query_information.insert("format".to_string(), READS_DEFAULT_FORMAT.to_string());
    }
    (Endpoint::Variants, None) => {
      query_information.insert("format".to_string(), VARIANTS_DEFAULT_FORMAT.to_string());
    }
    (Endpoint::Reads, Some(s)) if READS_FORMATS.contains(&s.as_str()) => (),
    (Endpoint::Variants, Some(s)) if VARIANTS_FORMATS.contains(&s.as_str()) => (),
    (_, Some(s)) => {
      return Err(HtsGetError::UnsupportedFormat(format!(
        "{} isn't a supported format",
        s
      )))
    }
  }
  let query = convert_to_query(&query_information)?;
  searcher
    .search(query)
    .map_err(|error| error.into())
    .map(JsonResponse::from_response)
}

fn convert_to_query(query_information: &HashMap<String, String>) -> Result<Query> {
  Ok(
    QueryBuilder::new(query_information.get("id"))?
      .with_format(query_information.get("format"))?
      .with_class(query_information.get("class"))?
      .with_reference_name(query_information.get("referenceName"))
      .with_range(query_information.get("start"), query_information.get("end"))?
      .with_fields(query_information.get("fields"))
      .with_tags(
        query_information.get("tags"),
        query_information.get("notags"),
      )?
      .build(),
  )
}

/// Gets a response in JSON for a POST request.
/// The parameters can be consulted [here](https://samtools.github.io/hts-specs/htsget.html)
pub fn get_response_for_post_request<H: HtsGet>(
  searcher: &H,
  mut request: PostRequest,
  id: impl Into<String>,
  endpoint: Endpoint,
) -> Result<String> {
  match (endpoint, &request.format) {
    (Endpoint::Reads, None) => request.format = Some(READS_DEFAULT_FORMAT.to_string()),
    (Endpoint::Variants, None) => request.format = Some(VARIANTS_DEFAULT_FORMAT.to_string()),
    (Endpoint::Reads, Some(s)) if READS_FORMATS.contains(&s.as_str()) => (),
    (Endpoint::Variants, Some(s)) if VARIANTS_FORMATS.contains(&s.as_str()) => (),
    (_, Some(s)) => {
      return Err(HtsGetError::UnsupportedFormat(format!(
        "{} isn't a supported format",
        s
      )))
    }
  }
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

fn merge_responses(responses: Vec<Response>) -> Option<Response> {
  responses.into_iter().reduce(|mut acc, mut response| {
    acc.urls.append(&mut response.urls);
    acc
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use htsget_search::{
    htsget::{from_storage::HtsGetFromStorage, Format, Headers, Url},
    storage::local::LocalStorage,
  };
  use std::path::PathBuf;
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
    HtsGetFromStorage::new(LocalStorage::new("../data").unwrap())
  }
}
