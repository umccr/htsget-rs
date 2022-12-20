use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use futures::future::join_all;
use futures::TryStreamExt;
use htsget_config::regex_resolver::LocalResolver;
use htsget_config::{Class, Format};
use http::Method;
use noodles_bgzf as bgzf;
use noodles_vcf as vcf;
use reqwest::ClientBuilder;
use tokio::time::sleep;

use htsget_http_core::{get_service_info_with, Endpoint};
use htsget_search::htsget::Response as HtsgetResponse;
use htsget_search::htsget::{Headers, JsonResponse, Url};
use htsget_search::storage::data_server::HttpTicketFormatter;

use crate::http_tests::{Header, Response, TestRequest, TestServer};
use crate::util::expected_bgzf_eof_data_url;
use crate::Config;

/// Test response with with class.
pub async fn test_response(response: Response, class: Class) {
  println!("response: {:?}", response);
  assert!(response.is_success());
  let body = response.deserialize_body::<JsonResponse>().unwrap();

  println!("{:#?}", body);
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
        .await.unwrap()
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
  let mut formatter = formatter_from_config(config).unwrap();
  spawn_ticket_server(config.data_server().unwrap().local_path().into(), &mut formatter).await;

  (expected_url_path(&formatter), formatter)
}

/// Get the expected url path from the formatter.
pub fn expected_url_path(formatter: &HttpTicketFormatter) -> String {
  format!("{}://{}", formatter.get_scheme(), formatter.get_addr())
}

/// Spawn the [TicketServer] using the path and formatter.
pub async fn spawn_ticket_server(path: PathBuf, formatter: &mut HttpTicketFormatter) {
  let server = formatter.bind_data_server().await.unwrap();
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

  test_response(response, Class::Body).await;
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

  test_response(response, Class::Body).await;
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

  test_response(response, Class::Body).await;
}

/// A parameterized post test with header as the class.
pub async fn test_parameterized_post_class_header<T: TestRequest>(tester: &impl TestServer<T>) {
  let request = post_request(tester).set_payload(
    "{\"format\": \"VCF\", \"class\": \"header\", \"regions\": [{\"referenceName\": \"chrM\"}]}",
  );
  let response = tester.test_server(request).await;
  test_response(response, Class::Header).await;
}

/// Get the [HttpTicketFormatter] from the config.
pub fn formatter_from_config(config: &Config) -> Option<HttpTicketFormatter> {
  HttpTicketFormatter::try_from(config.data_server().unwrap().clone()).ok()
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
    .with_headers(Headers::new(headers));
  let urls = match class {
    Class::Header => vec![http_url.with_class(Class::Header)],
    Class::Body => vec![http_url, Url::new(expected_bgzf_eof_data_url())],
  };

  JsonResponse::from(HtsgetResponse::new(Format::Vcf, urls))
}
