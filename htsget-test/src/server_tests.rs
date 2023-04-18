use std::fmt::Debug;
use std::net::SocketAddr;
use std::str::FromStr;

use crate::util::expected_bgzf_eof_data_url;
use base64::engine::general_purpose;
use base64::Engine;
use futures::future::join_all;
use futures::TryStreamExt;
use htsget_config::types::Format;
use http::header::HeaderName;
use http::{HeaderMap, HeaderValue, Method};
use noodles_bgzf as bgzf;
use noodles_vcf as vcf;
use reqwest::ClientBuilder;
use serde::Deserialize;
use serde_json::{json, Value};

use htsget_config::types::Class;

use crate::http_tests::{Header, Response, TestRequest, TestServer};
use crate::Config;

/// Test response with with class.
pub async fn test_response<R>(response: Response, class: Class)
where
  R: for<'de> Deserialize<'de> + Eq + Debug,
{
  println!("response: {response:?}");
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

  let merged_response = join_all(
    expected_response
      .get("htsget")
      .unwrap()
      .get("urls")
      .unwrap()
      .as_array()
      .unwrap()
      .iter()
      .map(|url| async {
        if let Some(data_uri) = url
          .get("url")
          .unwrap()
          .as_str()
          .unwrap()
          .strip_prefix("data:;base64,")
        {
          general_purpose::STANDARD.decode(data_uri).unwrap()
        } else {
          client
            .get(url.get("url").unwrap().as_str().unwrap())
            .headers(HeaderMap::from_iter(
              url
                .get("headers")
                .unwrap()
                .as_object()
                .unwrap_or(&serde_json::Map::new())
                .into_iter()
                .map(|(key, value)| {
                  (
                    HeaderName::from_str(key).unwrap(),
                    HeaderValue::from_str(value.as_str().unwrap()).unwrap(),
                  )
                }),
            ))
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .to_vec()
        }
      }),
  )
  .await
  .into_iter()
  .reduce(|acc, x| [acc, x].concat())
  .unwrap();

  let mut reader = vcf::AsyncReader::new(bgzf::AsyncReader::new(merged_response.as_slice()));
  let header = reader.read_header().await.unwrap().parse().unwrap();
  println!("{header}");

  let mut records = reader.records(&header);
  while let Some(record) = records.try_next().await.unwrap() {
    println!("{record}");
    continue;
  }
}

/// Get the expected url path from the formatter.
pub fn expected_url_path(config: &Config, local_addr: SocketAddr) -> String {
  let scheme = match config.data_server().tls() {
    None => "http",
    Some(_) => "https",
  };
  format!("{}://{}", scheme, local_addr)
}

/// Test response with with service info.
pub fn test_response_service_info(response: &Response) {
  let expected = json!({
    "id": "",
    "name": "",
    "version": "",
    "organization": {
      "name": "",
      "url": "",
    },
    "type": {
      "group": "",
      "artifact": "",
      "version": "",
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
    "contactUrl": "",
    "documentationUrl": "",
    "createdAt": "",
    "updatedAt": "",
    "environment": "",
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
        .get_request()
        .method(Method::GET.to_string())
        .uri("/variants/1-vcf/sample1-bcbio-cancer"),
      tester
        .get_request()
        .method(Method::GET.to_string())
        .uri("/variants/2-vcf/sample1-bcbio-cancer"),
    ],
    Class::Body,
  )
  .await;
}

fn post_request_one<T: TestRequest>(tester: &impl TestServer<T>) -> T {
  tester
    .get_request()
    .method(Method::POST.to_string())
    .uri("/variants/1-vcf/sample1-bcbio-cancer")
    .insert_header(Header {
      name: http::header::CONTENT_TYPE.to_string(),
      value: mime::APPLICATION_JSON.to_string(),
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
        .get_request()
        .method(Method::GET.to_string())
        .uri("/variants/1-vcf/sample1-bcbio-cancer?format=VCF&class=header"),
      tester
        .get_request()
        .method(Method::GET.to_string())
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
    .get_request()
    .method(Method::GET.to_string())
    .uri("/variants/service-info");
  let response = tester
    .test_server(request, tester.get_expected_path().await)
    .await;

  test_response_service_info(&response);
}

/// An example VCF search response.
pub fn expected_response(class: Class, url_path: String) -> Value {
  let url = format!("{url_path}/data/vcf/sample1-bcbio-cancer.vcf.gz");
  let headers = vec!["Range", "bytes=0-3465"];

  let urls = match class {
    Class::Header => json!([{
      "url": url,
      "headers": {
        headers[0]: headers[1]
      },
      "class": "header"
    }]),
    Class::Body => json!([{
      "url": url,
      "headers": {
        headers[0]: headers[1]
      },
    }, {
      "url": expected_bgzf_eof_data_url()
    }]),
  };

  json!({
    "htsget": {
      "format": Format::Vcf,
      "urls": urls
      }
  })
}
