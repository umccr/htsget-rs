use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use futures::future::join_all;
use futures::TryStreamExt;
use http::Method;
use serde::{de, Serialize};
use serde::Deserialize;
use reqwest::Client;
use reqwest::ClientBuilder;

use htsget_config::config::Config;
use htsget_http_core::{Endpoint, get_service_info_with, JsonResponse, JsonUrl};
use htsget_search::htsget::{Class, Format, Headers, Url};
use htsget_search::htsget::Class::Body;
use htsget_search::htsget::Response as HtsgetResponse;
use htsget_search::storage::ticket_server::HttpTicketFormatter;
use noodles_vcf as vcf;
use noodles_bgzf as bgzf;

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

  fn get_formatter(&self) -> HttpTicketFormatter {
    formatter_from_config(self.get_config())
  }
}

/// Test response with with class.
pub async fn test_response(response: &Response, config: Config, class: Class, formatter: HttpTicketFormatter) {
  let url_path = expected_local_storage_path(&config);
  let expected_response = expected_response(class, url_path);
  println!("{:?}", response);
  assert!(response.is_success());
  assert_eq!(
    expected_response,
    response.deserialize_body().unwrap()
  );

  let local_server = formatter.bind_ticket_server().await.unwrap();
  tokio::spawn(async move { local_server.serve(&config.path).await });

  let client = ClientBuilder::new()
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
            .unwrap_or(&HashMap::default())
            .try_into()
            .unwrap(),
        )
        .send().await.unwrap()
        .bytes().await.unwrap()
        .to_vec()
    }
  })).await.into_iter().reduce(|acc, x| [acc, x].concat()).unwrap();
  let mut reader = vcf::AsyncReader::new(bgzf::AsyncReader::new(merged_response.as_slice()));
  let header = reader.read_header().await.unwrap().parse().unwrap();
  println!("{}", header);

  let mut records = reader.records(&header);
  while let Some(record) = records.try_next().await.unwrap() {
    println!("{}", record);
    continue
  }
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
  test_response(&response, tester.get_config().clone(), Body, tester.get_formatter()).await;
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
  test_response(&response, tester.get_config().clone(), Body, tester.get_formatter()).await;
}

/// A parameterized get test.
pub async fn test_parameterized_get<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = tester
    .get_request()
    .method(Method::GET.to_string())
    .uri("/variants/vcf/sample1-bcbio-cancer?format=VCF&class=header");
  let response = tester.test_server(request).await;
  test_response(&response, tester.get_config().clone(), Class::Header, tester.get_formatter()).await;
}

/// A parameterized post test.
pub async fn test_parameterized_post<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = post_request(tester)
    .set_payload("{\"format\": \"VCF\", \"regions\": [{\"referenceName\": \"chrM\"}]}");
  let response = tester.test_server(request).await;
  test_response(&response, tester.get_config().clone(), Body, tester.get_formatter()).await;
}

/// A parameterized post test with header as the class.
pub async fn test_parameterized_post_class_header<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = post_request(tester).set_payload(
    "{\"format\": \"VCF\", \"class\": \"header\", \"regions\": [{\"referenceName\": \"chrM\"}]}",
  );
  let response = tester.test_server(request).await;
  test_response(&response, tester.get_config().clone(), Class::Header, tester.get_formatter()).await;
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
  match (&config.ticket_server_cert, &config.ticket_server_key) {
    (Some(_), Some(_)) => format!("https://{}", config.ticket_server_addr),
    (Some(_), None) | (None, Some(_)) => panic!("Both the cert and key must be provided."),
    (None, None) => format!("http://{}", config.ticket_server_addr),
  }
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

  JsonResponse::from_response(HtsgetResponse::new(Format::Vcf, urls))
}

/// Get the default directory where data is present.
pub fn default_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .unwrap()
    .to_path_buf()
}

fn set_path() {
  std::env::set_var("HTSGET_PATH", default_dir().join("data"));
}

/// Get the [HttpTicketFormatter] from th config.
pub fn formatter_from_config(config: &Config) -> HttpTicketFormatter {
  HttpTicketFormatter::try_from(
    config.ticket_server_addr,
    config.ticket_server_cert.clone(),
    config.ticket_server_key.clone(),
  ).unwrap()
}

/// Default config using the current cargo manifest directory.
pub fn default_test_config() -> Config {
  set_path();
  Config::from_env().expect("Expected valid environment variables.")
}

/// Config with tls ticket server, using the current cargo manifest directory.
pub fn test_config_with_tls<P: AsRef<Path>>(path: P) -> Config {
  set_path();

  let (key_path, cert_path) = generate_test_certificates(path, "key.pem", "cert.pem");
  std::env::set_var("HTSGET_TICKET_SERVER_KEY", key_path);
  std::env::set_var("HTSGET_TICKET_SERVER_CERT", cert_path);

  Config::from_env().expect("Expected valid environment variables.")
}

/// Get the event associated with the file.
pub fn get_test_file<P: AsRef<Path>>(path: P) -> String {
  let path = default_dir().join("data").join(path);
  fs::read_to_string(path).expect("Failed to read file.")
}
