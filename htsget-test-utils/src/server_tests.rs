use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use http::Method;
use serde::de;
use serde::Deserialize;

use htsget_config::config::Config;
use htsget_http_core::{get_service_info_with, Endpoint, JsonResponse};
use htsget_search::htsget::Class::Body;
use htsget_search::htsget::Response as HtsgetResponse;
use htsget_search::htsget::{Class, Format, Headers, Url};

use crate::util::expected_bgzf_eof_data_url;

/// Represents a http header.
#[derive(Debug)]
pub struct Header<T: Into<String>> {
  pub name: T,
  pub value: T,
}

impl<T: Into<String>> Header<T> {
  pub fn into_tuple(self) -> (String, String) {
    (self.name.into(), self.value.into())
  }
}

/// Represents a http response.
#[derive(Debug, Deserialize)]
pub struct Response {
  #[serde(alias = "statusCode")]
  pub status: u16,
  #[serde(with = "serde_bytes")]
  pub body: Vec<u8>,
}

impl Response {
  pub fn new(status: u16, body: Vec<u8>) -> Self {
    Self { status, body }
  }

  /// Deserialize the body from a slice.
  pub fn deserialize_body<T>(&self) -> Result<T, serde_json::Error>
  where
    T: de::DeserializeOwned,
  {
    serde_json::from_slice(&self.body)
  }

  /// Check if status code is success.
  pub fn is_success(&self) -> bool {
    300 > self.status && self.status >= 200
  }
}

/// Mock request trait that should be implemented to use test functions.
pub trait TestRequest {
  fn insert_header(self, header: Header<impl Into<String>>) -> Self;
  fn set_payload(self, payload: impl Into<String>) -> Self;
  fn uri(self, uri: impl Into<String>) -> Self;
  fn method(self, method: impl Into<String>) -> Self;
}

/// Mock server trait that should be implemented to use test functions.
#[async_trait(?Send)]
pub trait TestServer<T: TestRequest> {
  fn get_config(&self) -> &Config;
  fn get_request(&self) -> T;
  async fn test_server(&self, request: T) -> Response;
}

/// Test response with with class.
pub async fn test_response(response: &Response, config: &Config, class: Class) {
  let url_path = expected_local_storage_path(config);
  assert!(response.is_success());
  assert_eq!(
    expected_response(class, url_path).await,
    response.deserialize_body().unwrap()
  );
}

/// Test response with with service info.
pub fn test_response_service_info(response: &Response) {
  let expected = get_service_info_with(
    Endpoint::Variants,
    &[Format::Vcf, Format::Bcf],
    false,
    false,
  );
  assert!(response.is_success());
  assert_eq!(expected, response.deserialize_body().unwrap());
}

/// A get test using the tester.
pub async fn test_get<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = tester
    .get_request()
    .method(Method::GET.to_string())
    .uri("/variants/vcf/sample1-bcbio-cancer");
  let response = tester.test_server(request).await;
  test_response(&response, tester.get_config(), Class::Body).await;
}

fn post_request<T: TestRequest>(tester: &impl TestServer<T>) -> T {
  tester
    .get_request()
    .method(Method::POST.to_string())
    .uri("/variants/vcf/sample1-bcbio-cancer")
    .insert_header(Header {
      name: http::header::CONTENT_TYPE.to_string(),
      value: mime::APPLICATION_JSON.to_string(),
    })
}

/// A post test using the tester.
pub async fn test_post<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = post_request(tester).set_payload("{}");
  let response = tester.test_server(request).await;
  test_response(&response, tester.get_config(), Class::Body).await;
}

/// A parameterized get test.
pub async fn test_parameterized_get<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = tester
    .get_request()
    .method(Method::GET.to_string())
    .uri("/variants/vcf/sample1-bcbio-cancer?format=VCF&class=header");
  let response = tester.test_server(request).await;
  test_response(&response, tester.get_config(), Class::Header).await;
}

/// A parameterized post test.
pub async fn test_parameterized_post<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = post_request(tester)
    .set_payload("{\"format\": \"VCF\", \"regions\": [{\"referenceName\": \"chrM\"}]}");
  let response = tester.test_server(request).await;
  test_response(&response, tester.get_config(), Class::Body).await;
}

/// A parameterized post test with header as the class.
pub async fn test_parameterized_post_class_header<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = post_request(tester).set_payload(
    "{\"format\": \"VCF\", \"class\": \"header\", \"regions\": [{\"referenceName\": \"chrM\"}]}",
  );
  let response = tester.test_server(request).await;
  test_response(&response, tester.get_config(), Class::Header).await;
}

/// A service info test.
pub async fn test_service_info<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = tester
    .get_request()
    .method(Method::GET.to_string())
    .uri("/variants/service-info");
  let response = tester.test_server(request).await;
  test_response_service_info(&response);
}

fn expected_local_storage_path(config: &Config) -> String {
  format!("https://{}", config.ticket_server_addr)
}

/// An example VCF search response.
pub async fn expected_response(class: Class, url_path: String) -> JsonResponse {
  let mut headers = HashMap::new();
  headers.insert("Range".to_string(), "bytes=0-3366".to_string());

  let http_url = Url::new(format!("{}/data/vcf/sample1-bcbio-cancer.vcf.gz", url_path)).await
    .with_headers(Headers::new(headers))
    .with_class(class.clone());
  let urls = match class {
    Class::Header => vec![http_url],
    Class::Body => vec![
      http_url,
      Url::new(expected_bgzf_eof_data_url()).with_class(Body),
    ],
  };

  JsonResponse::from_response(HtsgetResponse::new(Format::Vcf, urls))
}

/// Get the default directory where data is present.
pub fn default_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .unwrap()
    .to_path_buf()
}

/// Default config using the current cargo manifest directory.
pub fn default_test_config() -> Config {
  std::env::set_var("HTSGET_PATH", default_dir().join("data"));
  Config::from_env().expect("Expected valid environment variables.")
}

/// Get the event associated with the file.
pub fn get_test_file<P: AsRef<Path>>(path: P) -> String {
  let path = default_dir().join("data").join(path);
  fs::read_to_string(path).expect("Failed to read file.")
}
