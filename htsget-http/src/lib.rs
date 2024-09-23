use std::result;
use std::str::FromStr;

pub use error::{HtsGetError, Result};
pub use htsget_config::config::{
  Config, DataServerConfig, ServiceInfo as ConfigServiceInfo, TicketServerConfig,
};
pub use htsget_config::storage::Storage;
use htsget_config::types::Format::{Bam, Bcf, Cram, Vcf};
use htsget_config::types::{Format, Query, Request, Response};
pub use http_core::{get, post};
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

/// A enum to distinguish between the two endpoint defined in the
/// [HtsGet specification](https://samtools.github.io/hts-specs/htsget.html)
#[derive(Debug, PartialEq, Eq)]
pub enum Endpoint {
  Reads,
  Variants,
}

impl FromStr for Endpoint {
  type Err = ();

  fn from_str(s: &str) -> result::Result<Self, Self::Err> {
    match s {
      "reads" => Ok(Self::Reads),
      "variants" => Ok(Self::Variants),
      _ => Err(()),
    }
  }
}

/// Get the format from the string
pub fn match_format(endpoint: &Endpoint, format: Option<impl Into<String>>) -> Result<Format> {
  let format = format.map(Into::into).map(|format| format.to_lowercase());

  match (endpoint, format) {
    (Endpoint::Reads, None) => Ok(Bam),
    (Endpoint::Variants, None) => Ok(Vcf),
    (Endpoint::Reads, Some(s)) if s == "bam" => Ok(Bam),
    (Endpoint::Reads, Some(s)) if s == "cram" => Ok(Cram),
    (Endpoint::Variants, Some(s)) if s == "vcf" => Ok(Vcf),
    (Endpoint::Variants, Some(s)) if s == "bcf" => Ok(Bcf),
    (_, Some(format)) => Err(HtsGetError::UnsupportedFormat(format!(
      "{format} isn't a supported format for this endpoint"
    ))),
  }
}

fn convert_to_query(request: Request, format: Format) -> Result<Query> {
  let query = request.query().clone();

  Ok(
    QueryBuilder::new(request, format)
      .with_class(query.get("class"))?
      .with_reference_name(query.get("referenceName"))
      .with_range(query.get("start"), query.get("end"))?
      .with_fields(query.get("fields"))
      .with_tags(query.get("tags"), query.get("notags"))?
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
  use std::collections::HashMap;
  use std::path::PathBuf;

  use http::uri::Authority;

  use htsget_config::storage::local::LocalStorage as ConfigLocalStorage;
  use htsget_config::types::{Headers, JsonResponse, Request, Scheme, Url};
  use htsget_search::from_storage::HtsGetFromStorage;
  use htsget_search::HtsGet;
  use htsget_search::LocalStorage;
  use htsget_search::Storage;

  use super::*;

  #[test]
  fn match_with_invalid_format() {
    assert!(matches!(
      match_format(&Endpoint::Reads, Some("Invalid".to_string())).unwrap_err(),
      HtsGetError::UnsupportedFormat(_)
    ));
  }

  #[test]
  fn match_with_invalid_endpoint() {
    assert!(matches!(
      match_format(&Endpoint::Variants, Some("bam".to_string())).unwrap_err(),
      HtsGetError::UnsupportedFormat(_)
    ));
  }

  #[test]
  fn match_with_valid_format() {
    assert!(matches!(
      match_format(&Endpoint::Reads, Some("bam".to_string())).unwrap(),
      Bam,
    ));
  }

  #[tokio::test]
  async fn get_request() {
    let request = HashMap::new();

    let mut expected_response_headers = Headers::default();
    expected_response_headers.insert("Range".to_string(), "bytes=0-2596798".to_string());

    let request = Request::new(
      "bam/htsnexus_test_NA12878".to_string(),
      request,
      Default::default(),
    );

    assert_eq!(
      get(get_searcher(), request, Endpoint::Reads).await,
      Ok(expected_bam_json_response(expected_response_headers))
    );
  }

  #[tokio::test]
  async fn get_reads_request_with_variants_format() {
    let mut request = HashMap::new();
    request.insert("format".to_string(), "VCF".to_string());

    let request = Request::new(
      "bam/htsnexus_test_NA12878".to_string(),
      request,
      Default::default(),
    );

    assert!(matches!(
      get(get_searcher(), request, Endpoint::Reads).await,
      Err(HtsGetError::UnsupportedFormat(_))
    ));
  }

  #[tokio::test]
  async fn get_request_with_range() {
    let mut request = HashMap::new();
    request.insert("referenceName".to_string(), "chrM".to_string());
    request.insert("start".to_string(), "149".to_string());
    request.insert("end".to_string(), "200".to_string());

    let mut expected_response_headers = Headers::default();
    expected_response_headers.insert("Range".to_string(), "bytes=0-3493".to_string());

    let request = Request::new(
      "vcf/sample1-bcbio-cancer".to_string(),
      request,
      Default::default(),
    );

    assert_eq!(
      get(get_searcher(), request, Endpoint::Variants).await,
      Ok(expected_vcf_json_response(expected_response_headers))
    );
  }

  #[tokio::test]
  async fn post_request() {
    let request = Request::new_with_id("bam/htsnexus_test_NA12878".to_string());
    let body = PostRequest {
      format: None,
      class: None,
      fields: None,
      tags: None,
      notags: None,
      regions: None,
    };

    let mut expected_response_headers = Headers::default();
    expected_response_headers.insert("Range".to_string(), "bytes=0-2596798".to_string());

    assert_eq!(
      post(get_searcher(), body, request, Endpoint::Reads).await,
      Ok(expected_bam_json_response(expected_response_headers))
    );
  }

  #[tokio::test]
  async fn post_variants_request_with_reads_format() {
    let request = Request::new_with_id("bam/htsnexus_test_NA12878".to_string());
    let body = PostRequest {
      format: Some("BAM".to_string()),
      class: None,
      fields: None,
      tags: None,
      notags: None,
      regions: None,
    };

    assert!(matches!(
      post(get_searcher(), body, request, Endpoint::Variants).await,
      Err(HtsGetError::UnsupportedFormat(_))
    ));
  }

  #[tokio::test]
  async fn post_request_with_range() {
    let request = Request::new_with_id("vcf/sample1-bcbio-cancer".to_string());
    let body = PostRequest {
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

    let mut expected_response_headers = Headers::default();
    expected_response_headers.insert("Range".to_string(), "bytes=0-3493".to_string());

    assert_eq!(
      post(get_searcher(), body, request, Endpoint::Variants).await,
      Ok(expected_vcf_json_response(expected_response_headers))
    );
  }

  fn expected_vcf_json_response(headers: Headers) -> JsonResponse {
    JsonResponse::from(Response::new(
      Vcf,
      vec![
        Url::new("http://127.0.0.1:8081/data/vcf/sample1-bcbio-cancer.vcf.gz".to_string())
          .with_headers(headers),
      ],
    ))
  }

  fn expected_bam_json_response(headers: Headers) -> JsonResponse {
    JsonResponse::from(Response::new(
      Bam,
      vec![
        Url::new("http://127.0.0.1:8081/data/bam/htsnexus_test_NA12878.bam".to_string())
          .with_headers(headers),
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

  fn get_searcher() -> impl HtsGet + Clone {
    HtsGetFromStorage::new(Storage::new(
      LocalStorage::new(
        get_base_path(),
        ConfigLocalStorage::new(
          Scheme::Http,
          Authority::from_static("127.0.0.1:8081"),
          "data".to_string(),
          "/data".to_string(),
          Default::default(),
          false,
        ),
      )
      .unwrap(),
    ))
  }
}
