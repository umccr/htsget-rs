use std::collections::HashMap;
use std::path::Path;
use http::{Method, StatusCode};
use htsget_http_core::{Endpoint, get_service_info_with, JsonResponse};
use htsget_search::htsget::{Class, Format, Headers, Query, Url};
use htsget_search::htsget::Class::Body;
use crate::{Header, HtsgetResponse, TestRequest, TestServer};

pub async fn test_get<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = tester.get_request().method(Method::GET.to_string()).uri("/variants/data/vcf/sample1-bcbio-cancer");
  let response = tester.test_server(request).await;
  assert!(response.is_success());
  assert_eq!(expected_response(&tester.get_config().htsget_path, Class::Body), response.deserialize_body().unwrap());
}

fn post_request<T: TestRequest>(tester: &impl TestServer<T>) -> T {
  tester.get_request().method(Method::POST.to_string()).uri("/variants/data/vcf/sample1-bcbio-cancer")
    .insert_header(Header {name: http::header::CONTENT_TYPE.to_string(), value: mime::APPLICATION_JSON.to_string() })
}

pub async fn test_post<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = post_request(tester).set_payload("{}");
  let response = tester.test_server(request).await;
  assert!(response.is_success());
  assert_eq!(expected_response(&tester.get_config().htsget_path, Class::Body), response.deserialize_body().unwrap());
}

pub async fn test_parameterized_get<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = tester.get_request().method(Method::GET.to_string()).uri("/variants/data/vcf/sample1-bcbio-cancer?format=VCF&class=header");
  let response = tester.test_server(request).await;
  assert!(response.is_success());
  assert_eq!(expected_response(&tester.get_config().htsget_path, Class::Header), response.deserialize_body().unwrap());
}

pub async fn test_parameterized_post<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = post_request(tester)
    .set_payload("{\"format\": \"VCF\", \"regions\": [{\"referenceName\": \"chrM\"}]}");
  let response = tester.test_server(request).await;
  assert!(response.is_success());
  assert_eq!(expected_response(&tester.get_config().htsget_path, Class::Body), response.deserialize_body().unwrap());
}

pub async fn test_service_info<T: TestRequest>(tester: &impl TestServer<T>) {
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


pub fn expected_response(path: &Path, class: Class) -> JsonResponse {
  let mut headers = HashMap::new();
  headers.insert("Range".to_string(), "bytes=0-3367".to_string());
  JsonResponse::from_response(HtsgetResponse::new(
    Format::Vcf,
    vec![Url::new(format!(
      "file://{}",
      path
        .join("data")
        .join("vcf")
        .join("sample1-bcbio-cancer.vcf.gz")
        .canonicalize()
        .unwrap()
        .to_string_lossy()
    ))
      .with_headers(Headers::new(headers))
      .with_class(class)],
  ))
}