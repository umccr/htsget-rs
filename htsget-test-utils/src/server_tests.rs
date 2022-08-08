use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use futures::future::join_all;
use futures::TryStreamExt;
use http::Method;
use noodles_bgzf as bgzf;
use noodles_vcf as vcf;
use reqwest::ClientBuilder;
use serde::de;
use serde::Deserialize;

use htsget_config::config::Config;
use htsget_http_core::{get_service_info_with, Endpoint};
use htsget_search::htsget::Class::Body;
use htsget_search::htsget::Response as HtsgetResponse;
use htsget_search::htsget::{Class, Format, Headers, Url, JsonResponse};
use htsget_search::storage::ticket_server::HttpTicketFormatter;

use crate::util::{expected_bgzf_eof_data_url, generate_test_certificates};

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
  pub expected_url_path: String,
}

impl Response {
  pub fn new(status: u16, body: Vec<u8>, expected_url_path: String) -> Self {
    Self {
      status,
      body,
      expected_url_path,
    }
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
pub async fn test_response(response: Response, class: Class) {
  assert!(response.is_success());
  let body = response.deserialize_body::<JsonResponse>().unwrap();
  let expected_response = expected_response(class, response.expected_url_path);
  assert_eq!(body, expected_response);

  let client = ClientBuilder::new()
    .danger_accept_invalid_certs(true)
    .use_rustls_tls()
    .build()
    .unwrap();

  let merged_response = join_all(expected_response.htsget.urls.iter().map(|url| async {
    if let Some(data_uri) = url.url.strip_prefix("data:;base64,") {
      base64::decode(data_uri).unwrap()
    } else {
      client
        .get(&url.url)
        .headers(
          url
            .headers
            .as_ref()
            .unwrap_or(&Headers::default())
            .as_ref_inner()
            .try_into()
            .unwrap(),
        )
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
        .to_vec()
    }
  }))
  .await
  .into_iter()
  .reduce(|acc, x| [acc, x].concat())
  .unwrap();

  let mut reader = vcf::AsyncReader::new(bgzf::AsyncReader::new(merged_response.as_slice()));
  let header = reader.read_header().await.unwrap().parse().unwrap();
  println!("{}", header);

  let mut records = reader.records(&header);
  while let Some(record) = records.try_next().await.unwrap() {
    println!("{}", record);
    continue;
  }
}

/// Create the a [HttpTicketFormatter], spawn the ticket server, returning the expected path and the formatter.
pub async fn formatter_and_expected_path(config: &Config) -> (String, HttpTicketFormatter) {
  let mut formatter = formatter_from_config(config);
  spawn_ticket_server(config.path.clone(), &mut formatter).await;

  (expected_url_path(&formatter), formatter)
}

/// Get the expected url path from the formatter.
pub fn expected_url_path(formatter: &HttpTicketFormatter) -> String {
  format!("{}://{}", formatter.get_scheme(), formatter.get_addr())
}

/// Spawn the [TicketServer] using the path and formatter.
pub async fn spawn_ticket_server(path: PathBuf, formatter: &mut HttpTicketFormatter) {
  let server = formatter.bind_ticket_server().await.unwrap();
  tokio::spawn(async move { server.serve(path).await.unwrap() });
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
  test_response(response, Body).await;
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
  test_response(response, Body).await;
}

/// A parameterized get test.
pub async fn test_parameterized_get<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = tester
    .get_request()
    .method(Method::GET.to_string())
    .uri("/variants/vcf/sample1-bcbio-cancer?format=VCF&class=header");
  let response = tester.test_server(request).await;
  test_response(response, Class::Header).await;
}

/// A parameterized post test.
pub async fn test_parameterized_post<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = post_request(tester)
    .set_payload("{\"format\": \"VCF\", \"regions\": [{\"referenceName\": \"chrM\"}]}");
  let response = tester.test_server(request).await;
  test_response(response, Body).await;
}

/// A parameterized post test with header as the class.
pub async fn test_parameterized_post_class_header<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = post_request(tester).set_payload(
    "{\"format\": \"VCF\", \"class\": \"header\", \"regions\": [{\"referenceName\": \"chrM\"}]}",
  );
  let response = tester.test_server(request).await;
  test_response(response, Class::Header).await;
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

/// An example VCF search response.
pub fn expected_response(class: Class, url_path: String) -> JsonResponse {
  let mut headers = HashMap::new();
  headers.insert("Range".to_string(), "bytes=0-3465".to_string());

  let http_url = Url::new(format!("{}/data/vcf/sample1-bcbio-cancer.vcf.gz", url_path))
    .with_headers(Headers::new(headers))
    .with_class(class.clone());
  let urls = match class {
    Class::Header => vec![http_url],
    Body => vec![
      http_url,
      Url::new(expected_bgzf_eof_data_url()).with_class(Body),
    ],
  };

  JsonResponse::from(HtsgetResponse::new(Format::Vcf, urls))
}

/// Get the default directory.
pub fn default_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .unwrap()
    .to_path_buf()
}

/// Get the default directory where data is present..
pub fn default_dir_data() -> PathBuf {
  default_dir().join("data")
}

fn set_path(config: &mut Config) {
  config.path = default_dir_data();
}

fn set_addr_and_path(config: &mut Config) {
  set_path(config);
  config.ticket_server_addr = "127.0.0.1:0".parse().unwrap();
}

/// Get the [HttpTicketFormatter] from the config.
pub fn formatter_from_config(config: &Config) -> HttpTicketFormatter {
  HttpTicketFormatter::try_from(
    config.ticket_server_addr,
    config.ticket_server_cert.clone(),
    config.ticket_server_key.clone(),
  )
  .unwrap()
}

/// Default config with fixed port.
pub fn default_config_fixed_port() -> Config {
  let mut config = Config::default();
  set_path(&mut config);
  config
}

/// Default config using the current cargo manifest directory, and dynamic port.
pub fn default_test_config() -> Config {
  let mut config = Config::default();
  set_addr_and_path(&mut config);
  config
}

/// Config with tls ticket server, using the current cargo manifest directory.
pub fn config_with_tls<P: AsRef<Path>>(path: P) -> Config {
  let mut config = Config::default();
  set_addr_and_path(&mut config);

  let (key_path, cert_path) = generate_test_certificates(path, "key.pem", "cert.pem");
  config.ticket_server_key = Some(key_path);
  config.ticket_server_cert = Some(cert_path);

  config
}

/// Get the event associated with the file.
pub fn get_test_file<P: AsRef<Path>>(path: P) -> String {
  let path = default_dir().join("data").join(path);
  fs::read_to_string(path).expect("Failed to read file.")
}
