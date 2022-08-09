use std::collections::HashMap;
use std::str::FromStr;

pub use error::{HtsGetError, Result};
use htsget_search::htsget::{Query, Response};
pub use http_core::{get_response_for_get_request, get_response_for_post_request};
pub use post_request::{PostRequest, Region};
use query_builder::QueryBuilder;
pub use service_info::get_service_info_json;
pub use service_info::get_service_info_with;
pub use service_info::{Htsget, Organisation, ServiceInfo, Type};

mod error;
mod http_core;
mod post_request;
mod query_builder;
mod service_info;

const READS_DEFAULT_FORMAT: &str = "BAM";
const VARIANTS_DEFAULT_FORMAT: &str = "VCF";
const READS_FORMATS: [&str; 2] = ["BAM", "CRAM"];
const VARIANTS_FORMATS: [&str; 2] = ["VCF", "BCF"];

/// A enum to distinguish between the two endpoint defined in the
/// [HtsGet specification](https://samtools.github.io/hts-specs/htsget.html)
#[derive(Debug, PartialEq)]
pub enum Endpoint {
  Reads,
  Variants,
}

impl FromStr for Endpoint {
  type Err = ();

  fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
    match s {
      "reads" => Ok(Self::Reads),
      "variants" => Ok(Self::Variants),
      _ => Err(()),
    }
  }
}

pub(crate) fn match_endpoints_get_request(
  endpoint: &Endpoint,
  query_information: &mut HashMap<String, String>,
) -> Result<()> {
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
        "{} isn't a supported format for this endpoint.",
        s
      )))
    }
  }
  Ok(())
}

pub(crate) fn match_endpoints_post_request(
  endpoint: &Endpoint,
  request: &mut PostRequest,
) -> Result<()> {
  match (endpoint, &request.format) {
    (Endpoint::Reads, None) => request.format = Some(READS_DEFAULT_FORMAT.to_string()),
    (Endpoint::Variants, None) => request.format = Some(VARIANTS_DEFAULT_FORMAT.to_string()),
    (Endpoint::Reads, Some(s)) if READS_FORMATS.contains(&s.as_str()) => (),
    (Endpoint::Variants, Some(s)) if VARIANTS_FORMATS.contains(&s.as_str()) => (),
    (_, Some(s)) => {
      return Err(HtsGetError::UnsupportedFormat(format!(
        "{} isn't a supported format for this endpoint.",
        s
      )))
    }
  }
  Ok(())
}

fn convert_to_query(query_information: &HashMap<String, String>) -> Result<Query> {
  Ok(
    QueryBuilder::new(query_information.get("id"), query_information.get("format"))?
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

fn merge_responses(responses: Vec<Response>) -> Option<Response> {
  responses.into_iter().reduce(|mut acc, mut response| {
    acc.urls.append(&mut response.urls);
    acc
  })
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;
  use std::sync::Arc;

  use htsget_config::regex_resolver::RegexResolver;
  use htsget_search::htsget::HtsGet;
  use htsget_search::storage::ticket_server::HttpTicketFormatter;
  use htsget_search::{
    htsget::{from_storage::HtsGetFromStorage, Format, Headers, JsonResponse, Url},
    storage::local::LocalStorage,
  };
  use htsget_test_utils::util::expected_bgzf_eof_data_url;

  use super::*;

  #[tokio::test]
  async fn get_request() {
    let mut request = HashMap::new();
    request.insert("id".to_string(), "bam/htsnexus_test_NA12878".to_string());
    let mut headers = HashMap::new();
    headers.insert("Range".to_string(), "bytes=0-2596770".to_string());
    assert_eq!(
      get_response_for_get_request(get_searcher(), request, Endpoint::Reads).await,
      Ok(example_bam_json_response(headers))
    );
  }

  #[tokio::test]
  async fn get_reads_request_with_variants_format() {
    let mut request = HashMap::new();
    request.insert("id".to_string(), "bam/htsnexus_test_NA12878".to_string());
    request.insert("format".to_string(), "VCF".to_string());
    assert!(matches!(
      get_response_for_get_request(get_searcher(), request, Endpoint::Reads).await,
      Err(HtsGetError::UnsupportedFormat(_))
    ));
  }

  #[tokio::test]
  async fn get_request_with_range() {
    let mut request = HashMap::new();
    request.insert("id".to_string(), "vcf/sample1-bcbio-cancer".to_string());
    request.insert("referenceName".to_string(), "chrM".to_string());
    request.insert("start".to_string(), "149".to_string());
    request.insert("end".to_string(), "200".to_string());
    let mut headers = HashMap::new();
    headers.insert("Range".to_string(), "bytes=0-3465".to_string());
    assert_eq!(
      get_response_for_get_request(get_searcher(), request, Endpoint::Variants).await,
      Ok(example_vcf_json_response(headers))
    );
  }

  #[tokio::test]
  async fn post_request() {
    let request = PostRequest {
      format: None,
      class: None,
      fields: None,
      tags: None,
      notags: None,
      regions: None,
    };
    let mut headers = HashMap::new();
    headers.insert("Range".to_string(), "bytes=0-2596770".to_string());
    assert_eq!(
      get_response_for_post_request(
        get_searcher(),
        request,
        "bam/htsnexus_test_NA12878",
        Endpoint::Reads
      )
      .await,
      Ok(example_bam_json_response(headers))
    );
  }

  #[tokio::test]
  async fn post_variants_request_with_reads_format() {
    let request = PostRequest {
      format: Some("BAM".to_string()),
      class: None,
      fields: None,
      tags: None,
      notags: None,
      regions: None,
    };
    assert!(matches!(
      get_response_for_post_request(
        get_searcher(),
        request,
        "bam/htsnexus_test_NA12878",
        Endpoint::Variants
      )
      .await,
      Err(HtsGetError::UnsupportedFormat(_))
    ));
  }

  #[tokio::test]
  async fn post_request_with_range() {
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
    headers.insert("Range".to_string(), "bytes=0-3465".to_string());
    assert_eq!(
      get_response_for_post_request(
        get_searcher(),
        request,
        "vcf/sample1-bcbio-cancer",
        Endpoint::Variants
      )
      .await,
      Ok(example_vcf_json_response(headers))
    );
  }

  fn example_vcf_json_response(headers: HashMap<String, String>) -> JsonResponse {
    JsonResponse::from(Response::new(
      Format::Vcf,
      vec![
        Url::new("http://127.0.0.1:8081/data/vcf/sample1-bcbio-cancer.vcf.gz".to_string())
          .with_headers(Headers::new(headers)),
        Url::new(expected_bgzf_eof_data_url()),
      ],
    ))
  }

  fn example_bam_json_response(headers: HashMap<String, String>) -> JsonResponse {
    JsonResponse::from(Response::new(
      Format::Bam,
      vec![
        Url::new("http://127.0.0.1:8081/data/bam/htsnexus_test_NA12878.bam".to_string())
          .with_headers(Headers::new(headers)),
        Url::new(expected_bgzf_eof_data_url()),
      ],
    ))
  }

  fn get_base_path() -> PathBuf {
    std::env::current_dir()
      .unwrap()
      .parent()
      .unwrap()
      .join("data")
  }

  fn get_searcher() -> Arc<impl HtsGet> {
    Arc::new(HtsGetFromStorage::new(
      LocalStorage::new(
        get_base_path(),
        RegexResolver::new(".*", "$0").unwrap(),
        HttpTicketFormatter::new("127.0.0.1:8081".parse().unwrap()),
      )
      .unwrap(),
    ))
  }
}
