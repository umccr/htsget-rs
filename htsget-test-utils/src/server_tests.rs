use std::collections::HashMap;
use std::path::{Path, PathBuf};

use http::Method;

use htsget_config::config::HtsgetConfig;
use htsget_http_core::{get_service_info_with, Endpoint, JsonResponse};
use htsget_search::htsget::{Class, Format, Headers, Url};

use crate::{Header, HtsgetResponse, TestRequest, TestServer};

/// A get test.
pub async fn test_get<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = tester
    .get_request()
    .method(Method::GET.to_string())
    .uri("/variants/data/vcf/sample1-bcbio-cancer");
  let response = tester.test_server(request).await;
  assert!(response.is_success());
  assert_eq!(
    expected_response(&tester.get_config().htsget_path, Class::Body),
    response.deserialize_body().unwrap()
  );
}

fn post_request<T: TestRequest>(tester: &impl TestServer<T>) -> T {
  tester
    .get_request()
    .method(Method::POST.to_string())
    .uri("/variants/data/vcf/sample1-bcbio-cancer")
    .insert_header(Header {
      name: http::header::CONTENT_TYPE.to_string(),
      value: mime::APPLICATION_JSON.to_string(),
    })
}

/// A post test.
pub async fn test_post<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = post_request(tester).set_payload("{}");
  let response = tester.test_server(request).await;
  assert!(response.is_success());
  assert_eq!(
    expected_response(&tester.get_config().htsget_path, Class::Body),
    response.deserialize_body().unwrap()
  );
}

/// A parameterized get test.
pub async fn test_parameterized_get<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = tester
    .get_request()
    .method(Method::GET.to_string())
    .uri("/variants/data/vcf/sample1-bcbio-cancer?format=VCF&class=header");
  let response = tester.test_server(request).await;
  assert!(response.is_success());
  assert_eq!(
    expected_response(&tester.get_config().htsget_path, Class::Header),
    response.deserialize_body().unwrap()
  );
}

/// A parameterized post test.
pub async fn test_parameterized_post<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = post_request(tester)
    .set_payload("{\"format\": \"VCF\", \"regions\": [{\"referenceName\": \"chrM\"}]}");
  let response = tester.test_server(request).await;
  assert!(response.is_success());
  assert_eq!(
    expected_response(&tester.get_config().htsget_path, Class::Body),
    response.deserialize_body().unwrap()
  );
}

/// A service info test.
pub async fn test_service_info<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = tester
    .get_request()
    .method(Method::GET.to_string())
    .uri("/variants/service-info");
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

/// An example VCF search response.
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

/// Default config using the current cargo manifest directory.
pub fn default_test_config() -> HtsgetConfig {
  std::env::set_var(
    "HTSGET_PATH",
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
  );
  envy::from_env::<HtsgetConfig>().expect("Expected valid environment variables.")
}
