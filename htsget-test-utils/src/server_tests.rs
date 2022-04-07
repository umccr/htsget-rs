use std::collections::HashMap;
use std::path::Path;
use http::{Method, StatusCode};
use htsget_http_core::JsonResponse;
use htsget_search::htsget::{Format, Headers, Url};
use crate::{HtsgetResponse, TestServer};

pub async fn test_get(tester: impl TestServer, path: &Path) {
  let request = tester.method(Method::GET.to_string()).uri("/variants/data/vcf/sample1-bcbio-cancer");
  let response = request.test_server().await;
  assert_eq!(response.status, StatusCode::OK.as_u16());
  assert_eq!(expected_response(path, None), response.body);
}

pub fn expected_response(path: &Path, headers: Option<Headers>) -> JsonResponse {
  let url = Url::new(format!(
    "file://{}",
    path
      .join("data")
      .join("vcf")
      .join("sample1-bcbio-cancer.vcf.gz")
      .to_string_lossy()
  ));
  if let Some(headers) = headers {
    JsonResponse::from_response(HtsgetResponse::new(
      Format::Vcf,
      vec![url.with_headers(headers)]),
    )
  } else {
    JsonResponse::from_response(HtsgetResponse::new(
      Format::Vcf,
      vec![url]),
    )
  }
}

pub fn expected_headers() -> Headers {
  let mut headers = HashMap::new();
  headers.insert("Range".to_string(), "bytes=0-3367".to_string());
  Headers::new(headers)
}