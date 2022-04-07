use std::collections::HashMap;
use std::path::Path;
use http::{Method, StatusCode};
use htsget_http_core::{Endpoint, get_service_info_with, JsonResponse};
use htsget_search::htsget::{Format, Headers, Query, Url};
use crate::{Header, HtsgetResponse, TestRequest, TestServer};

pub async fn test_get<T: TestRequest>(tester: &impl TestServer<T>, path: &Path) {
  let request = tester.get_request().method(Method::GET.to_string()).uri("/variants/data/vcf/sample1-bcbio-cancer");
  let response = tester.test_server(request).await;
  assert!(response.is_success());
  assert_eq!(expected_response(path, None), response.deserialize_body().unwrap());
}

pub async fn test_post<T: TestRequest>(tester: &impl TestServer<T>, path: &Path) {
  let request = tester.get_request().method(Method::POST.to_string()).uri("/variants/data/vcf/sample1-bcbio-cancer")
    .insert_header(Header {name: http::header::CONTENT_TYPE.to_string(), value: mime::APPLICATION_JSON.to_string() })
    .set_payload("{}");
  let response = tester.test_server(request).await;
  assert!(response.is_success());
  assert_eq!(expected_response(path, None), response.deserialize_body().unwrap());
}

async fn test_parameterized_get<T: TestRequest>(tester: &impl TestServer<T>, path: &Path) {
  let request = tester.get_request().method(Method::GET.to_string()).uri("/variants/data/vcf/sample1-bcbio-cancer?format=VCF&class=header");
  let response = tester.test_server(request).await;
  assert!(response.is_success());
  assert_eq!(expected_response(path, Some(expected_headers())), response.deserialize_body().unwrap());
}

async fn test_parameterized_post<T: TestRequest>(tester: &impl TestServer<T>, path: &Path) {
  let request = tester.get_request().method(Method::POST.to_string()).uri("/variants/data/vcf/sample1-bcbio-cancer")
    .insert_header(Header {name: http::header::CONTENT_TYPE.to_string(), value: mime::APPLICATION_JSON.to_string() })
    .set_payload("{\"format\": \"VCF\", \"regions\": [{\"referenceName\": \"chrM\"}]}");
  let response = tester.test_server(request).await;
  assert!(response.is_success());
  assert_eq!(expected_response(path, None), response.deserialize_body().unwrap());
}

async fn test_service_info<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = tester.get_request().method(Method::GET.to_string()).uri("/variants/service-info");
  let response = tester.test_server(request).await;
  let expected = get_service_info_with(
    Endpoint::Variants,
    &[Format::Vcf, Format::Bcf],
    false,
    false,
  );
  assert!(response.is_success());
  assert_eq!(expected, response.deserialize_body().unwrap());
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