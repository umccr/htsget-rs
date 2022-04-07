use std::collections::HashMap;
use std::path::Path;
use http::{Method, StatusCode};
use htsget_http_core::{Endpoint, get_service_info_with, JsonResponse};
use htsget_search::htsget::{Format, Headers, Query, Url};
use crate::{Header, HtsgetResponse, TestServer};

pub async fn test_get(tester: impl TestServer, path: &Path) {
  let response = tester.method(Method::GET.to_string()).uri("/variants/data/vcf/sample1-bcbio-cancer").test_server().await;
  assert_eq!(response.status, StatusCode::OK.as_u16());
  assert_eq!(expected_response(path, None), serde_json::from_slice(&response.body).unwrap());
}

pub async fn test_post(tester: impl TestServer, path: &Path) {
  let response = tester.method(Method::POST.to_string()).uri("/variants/data/vcf/sample1-bcbio-cancer")
    .insert_header(Header {name: http::header::CONTENT_TYPE.to_string(), value: mime::APPLICATION_JSON.to_string() })
    .set_payload("{}").test_server().await;
  assert_eq!(response.status, StatusCode::OK.as_u16());
  assert_eq!(expected_response(path, None), serde_json::from_slice(&response.body).unwrap());
}

async fn test_parameterized_get(tester: impl TestServer, path: &Path) {
  let response = tester.method(Method::GET.to_string()).uri("/variants/data/vcf/sample1-bcbio-cancer?format=VCF&class=header").test_server().await;
  assert_eq!(response.status, StatusCode::OK.as_u16());
  assert_eq!(expected_response(path, Some(expected_headers())), serde_json::from_slice(&response.body).unwrap());
}

async fn test_parameterized_post(tester: impl TestServer, path: &Path) {
  let response = tester.method(Method::POST.to_string()).uri("/variants/data/vcf/sample1-bcbio-cancer")
    .insert_header(Header {name: http::header::CONTENT_TYPE.to_string(), value: mime::APPLICATION_JSON.to_string() })
    .set_payload("{\"format\": \"VCF\", \"regions\": [{\"referenceName\": \"chrM\"}]}").test_server().await;
  assert_eq!(response.status, StatusCode::OK.as_u16());
  assert_eq!(expected_response(path, None), serde_json::from_slice(&response.body).unwrap());
}

async fn test_service_info(tester: impl TestServer, path: &Path) {
  let response = tester.method(Method::GET.to_string()).uri("/variants/service-info").test_server().await;
  let expected = get_service_info_with(
    Endpoint::Variants,
    &[Format::Vcf, Format::Bcf],
    false,
    false,
  );
  assert_eq!(response.status, StatusCode::OK.as_u16());
  assert_eq!(expected, serde_json::from_slice(&response.body).unwrap());
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