use std::fmt::Debug;
use std::net::SocketAddr;

use crate::http::concat::ConcatResponse;
use htsget_config::config::data_server::DataServerEnabled;
use htsget_config::config::Config;
use htsget_config::types::Class;
use htsget_config::types::Format;
use http::{HeaderValue, Method, StatusCode};
use reqwest::ClientBuilder;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::http::{Header, Response, TestRequest, TestServer};

/// Test response with with class.
pub async fn test_response<R>(response: Response, class: Class)
where
  R: for<'de> Deserialize<'de> + Eq + Debug,
{
  println!(
    "response body: {}",
    String::from_utf8_lossy(response.body.as_slice())
  );
  assert!(response.is_success());
  let body = response.deserialize_body::<R>().unwrap();

  let expected_response = expected_response(class, response.expected_url_path);
  assert_eq!(
    body,
    serde_json::from_value(expected_response.clone()).unwrap()
  );

  let client = ClientBuilder::new()
    .danger_accept_invalid_certs(true)
    .use_rustls_tls()
    .build()
    .unwrap();

  ConcatResponse::new(
    serde_json::from_value(expected_response.get("htsget").unwrap().clone()).unwrap(),
    class,
  )
  .concat_from_client(&client)
  .await
  .unwrap()
  .read_records()
  .await
  .unwrap();
}

/// Get the expected url path from the formatter.
pub fn expected_url_path(config: &Config, local_addr: SocketAddr) -> String {
  let mut scheme = "http";
  if let DataServerEnabled::Some(server) = config.data_server() {
    if server.tls().is_some() {
      scheme = "https";
    }
  }
  format!("{}://{}", scheme, local_addr)
}

/// Test response with with service info.
pub fn test_response_service_info(response: &Response) {
  let expected = json!({
    "type": {
      "group": "org.ga4gh",
      "artifact": "htsget",
      "version": "1.3.0",
    },
    "htsget": {
      "datatype": "variants",
      "formats": [
        "VCF",
        "BCF",
      ],
      "fieldsParametersEffective": false,
      "tagsParametersEffective": false,
    },
  });

  println!("{:#?}", expected);
  assert!(response.is_success());
  assert_eq!(expected, response.deserialize_body::<Value>().unwrap());
}

/// A get test using the tester.
pub async fn test_get<R, T>(tester: &impl TestServer<T>)
where
  T: TestRequest,
  R: for<'de> Deserialize<'de> + Eq + Debug,
{
  test_responses::<R, T>(
    tester,
    vec![
      tester
        .request()
        .method(Method::GET)
        .uri("/variants/1-vcf/sample1-bcbio-cancer"),
      tester
        .request()
        .method(Method::GET)
        .uri("/variants/2-vcf/sample1-bcbio-cancer"),
    ],
    Class::Body,
  )
  .await;
}

fn post_request_one<T: TestRequest>(tester: &impl TestServer<T>) -> T {
  tester
    .request()
    .method(Method::POST)
    .uri("/variants/1-vcf/sample1-bcbio-cancer")
    .insert_header(Header {
      name: http::header::CONTENT_TYPE,
      value: mime::APPLICATION_JSON
        .to_string()
        .parse::<HeaderValue>()
        .unwrap(),
    })
}

fn post_request_two<T: TestRequest>(tester: &impl TestServer<T>) -> T {
  post_request_one(tester).uri("/variants/2-vcf/sample1-bcbio-cancer")
}

/// Test an array of requests and their responses
async fn test_responses<R, T>(tester: &impl TestServer<T>, requests: Vec<T>, class: Class)
where
  T: TestRequest,
  R: for<'de> Deserialize<'de> + Eq + Debug,
{
  let expected_path = tester.get_expected_path().await;

  for request in requests.into_iter() {
    let response = tester.test_server(request, expected_path.clone()).await;
    test_response::<R>(response, class).await;
  }
}

/// Test an array of requests that are expected to return error status codes.
async fn test_error_response<T>(
  tester: &impl TestServer<T>,
  request: T,
  expected_status: StatusCode,
) where
  T: TestRequest,
{
  let response = tester.test_server(request, "".to_string()).await;
  assert_eq!(response.status, expected_status);
}

/// A post test using the tester.
pub async fn test_post<R, T>(tester: &impl TestServer<T>)
where
  T: TestRequest,
  R: for<'de> Deserialize<'de> + Eq + Debug,
{
  test_responses::<R, T>(
    tester,
    vec![
      post_request_one(tester).set_payload("{}"),
      post_request_two(tester).set_payload("{}"),
    ],
    Class::Body,
  )
  .await;
}

/// A parameterized get test.
pub async fn test_parameterized_get<R, T>(tester: &impl TestServer<T>)
where
  T: TestRequest,
  R: for<'de> Deserialize<'de> + Eq + Debug,
{
  test_responses::<R, T>(
    tester,
    vec![
      tester
        .request()
        .method(Method::GET)
        .uri("/variants/1-vcf/sample1-bcbio-cancer?format=VCF&class=header"),
      tester
        .request()
        .method(Method::GET)
        .uri("/variants/2-vcf/sample1-bcbio-cancer?format=VCF&class=header"),
    ],
    Class::Header,
  )
  .await;
}

/// A parameterized post test.
pub async fn test_parameterized_post<R, T>(tester: &impl TestServer<T>)
where
  T: TestRequest,
  R: for<'de> Deserialize<'de> + Eq + Debug,
{
  test_responses::<R, T>(
    tester,
    vec![
      post_request_one(tester)
        .set_payload("{\"format\": \"VCF\", \"regions\": [{\"referenceName\": \"chrM\"}]}"),
      post_request_two(tester)
        .set_payload("{\"format\": \"VCF\", \"regions\": [{\"referenceName\": \"chrM\"}]}"),
    ],
    Class::Body,
  )
  .await;
}

/// A parameterized post test with header as the class.
pub async fn test_parameterized_post_class_header<R, T>(tester: &impl TestServer<T>)
where
  T: TestRequest,
  R: for<'de> Deserialize<'de> + Eq + Debug,
{
  test_responses::<R, T>(
    tester,
    vec![
    post_request_one(tester).set_payload(
      "{\"format\": \"VCF\", \"class\": \"header\", \"regions\": [{\"referenceName\": \"chrM\"}]}",
    ),
    post_request_two(tester).set_payload(
      "{\"format\": \"VCF\", \"class\": \"header\", \"regions\": [{\"referenceName\": \"chrM\"}]}",
    )
  ],
    Class::Header,
  )
  .await;
}

/// A service info test.
pub async fn test_service_info<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = tester
    .request()
    .method(Method::GET)
    .uri("/variants/service-info");
  let response = tester
    .test_server(request, tester.get_expected_path().await)
    .await;

  test_response_service_info(&response);
}

/// Test requests that should result in errors.
pub async fn test_errors<T>(tester: &impl TestServer<T>)
where
  T: TestRequest,
{
  test_error_response(
    tester,
    tester.request().method(Method::DELETE).uri("/reads/id"),
    StatusCode::METHOD_NOT_ALLOWED,
  )
  .await;
  test_error_response(
    tester,
    tester.request().method(Method::GET).uri("/"),
    StatusCode::NOT_FOUND,
  )
  .await;
  test_error_response(
    tester,
    tester.request().method(Method::GET).uri("/path"),
    StatusCode::NOT_FOUND,
  )
  .await;
  test_error_response(
    tester,
    tester.request().method(Method::GET).uri("/reads"),
    StatusCode::NOT_FOUND,
  )
  .await;
  test_error_response(
    tester,
    tester.request().method(Method::DELETE).uri("/variants"),
    StatusCode::NOT_FOUND,
  )
  .await;

  test_error_response(
    tester,
    tester
      .request()
      .method(Method::GET)
      .uri("/variants/1-vcf/sample1-bcbio-cancer?format=BED"),
    StatusCode::BAD_REQUEST,
  )
  .await;
  test_error_response(
    tester,
    tester
      .request()
      .method(Method::GET)
      .uri("/variants/1-vcf/sample1-bcbio-cancer?class=header&start=1"),
    StatusCode::BAD_REQUEST,
  )
  .await;
  test_error_response(
    tester,
    tester
      .request()
      .method(Method::GET)
      .uri("/variants/1-vcf/sample1-bcbio-cancer?referenceName=*&start=1"),
    StatusCode::BAD_REQUEST,
  )
  .await;
  test_error_response(
    tester,
    tester
      .request()
      .method(Method::GET)
      .uri("/variants/1-vcf/sample1-bcbio-cancer?referenceName=chr1&start=2&end=1"),
    StatusCode::BAD_REQUEST,
  )
  .await;
  test_error_response(
    tester,
    tester
      .request()
      .method(Method::GET)
      .uri("/variants/1-vcf/sample1-bcbio-cancer?referenceName=*&end=1"),
    StatusCode::BAD_REQUEST,
  )
  .await;
}

/// An example VCF search response.
pub fn expected_response(class: Class, url_path: String) -> Value {
  let url = format!("{url_path}/vcf/sample1-bcbio-cancer.vcf.gz");

  let urls = match class {
    Class::Header => json!([{
      "url": url,
      "headers": {
        "Range": "bytes=0-3465"
      },
      "class": "header"
    }]),
    Class::Body => json!([{
      "url": url,
      "headers": {
        "Range": "bytes=0-3493"
      },
    }]),
  };

  json!({
    "htsget": {
      "format": Format::Vcf,
      "urls": urls
    }
  })
}
