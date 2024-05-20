use axum::body::StreamBody;
#[cfg(feature = "crypt4gh")]
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{get, head};
use axum::Router;
#[cfg(feature = "crypt4gh")]
use base64::engine::general_purpose;
#[cfg(feature = "crypt4gh")]
use base64::Engine;
#[cfg(feature = "crypt4gh")]
use crypt4gh::{encrypt, Keys};
#[cfg(feature = "crypt4gh")]
use std::collections::HashSet;
use std::fmt::Debug;
use std::future::Future;
use std::io::Cursor;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;

#[cfg(feature = "crypt4gh")]
use async_crypt4gh::util::read_public_key;
#[cfg(feature = "crypt4gh")]
use async_crypt4gh::{KeyPair, PublicKey};
use axum::extract::path::Path as AxumPath;
use htsget_config::types::Format;
#[cfg(feature = "crypt4gh")]
use http::header::RANGE;
use http::header::{CONTENT_LENGTH, USER_AGENT};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use reqwest::ClientBuilder;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
#[cfg(feature = "crypt4gh")]
use tokio_rustls::rustls::PrivateKey;
use tokio_util::io::ReaderStream;
use tower_http::services::ServeDir;
use walkdir::WalkDir;

use crate::http::concat::ConcatResponse;
use htsget_config::types::Class;

#[cfg(feature = "crypt4gh")]
use crate::crypt4gh::{expected_key_pair, test_auth};
use crate::http::{default_dir, Header, Response, TestRequest, TestServer};
use crate::Config;

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

pub async fn with_test_server<F, Fut>(server_base_path: &Path, test: F)
where
  F: FnOnce(String) -> Fut,
  Fut: Future<Output = ()>,
{
  let router = Router::new()
    .route(
      "/endpoint_file/:id",
      get(
        |headers: HeaderMap, AxumPath(_id): AxumPath<String>| async move {
          assert_eq!(
            headers.get(USER_AGENT),
            Some(&HeaderValue::from_static("user-agent"))
          );

          #[cfg(feature = "crypt4gh")]
          if headers.contains_key("client-public-key") {
            let entry = WalkDir::new(default_dir().join("data"))
              .min_depth(2)
              .into_iter()
              .filter_entry(|e| {
                e.path()
                  .file_name()
                  .map(|file| file.to_string_lossy() == _id.strip_suffix(".c4gh").unwrap_or(&_id))
                  .unwrap_or(false)
              })
              .filter_map(|v| v.ok())
              .next()
              .unwrap();

            let range = headers.get(RANGE).unwrap().to_str().unwrap();

            let range = range.replacen("bytes=", "", 1);

            let split: Vec<&str> = range.splitn(2, '-').collect();

            let parse_range = |range: Option<&str>| {
              let range = range.unwrap_or_default();
              if range.is_empty() {
                None
              } else {
                Some(range.parse().unwrap())
              }
            };

            let start: Option<u64> = parse_range(split.first().copied());
            let end: Option<u64> = parse_range(split.last().copied()).map(|value| value + 1);

            let mut bytes = vec![];
            let path = entry.path();
            File::open(path)
              .await
              .unwrap()
              .read_to_end(&mut bytes)
              .await
              .unwrap();

            let encryption_keys = KeyPair::new(
              PrivateKey(vec![
                161, 61, 174, 214, 146, 101, 139, 42, 247, 73, 68, 96, 8, 198, 29, 26, 68, 113,
                200, 182, 20, 217, 151, 89, 211, 14, 110, 80, 111, 138, 255, 194,
              ]),
              PublicKey::new(vec![
                249, 209, 232, 54, 131, 32, 40, 191, 15, 205, 151, 70, 90, 37, 149, 101, 55, 138,
                22, 59, 176, 0, 59, 7, 167, 10, 194, 129, 55, 147, 141, 101,
              ]),
            );

            let keys = Keys {
              method: 0,
              privkey: encryption_keys.private_key().clone().0,
              recipient_pubkey: read_public_key(
                general_purpose::STANDARD
                  .decode(headers.get("client-public-key").unwrap())
                  .unwrap(),
              )
              .await
              .unwrap()
              .into_inner(),
            };

            assert_eq!(
              keys.recipient_pubkey,
              expected_key_pair().public_key().clone().into_inner()
            );

            let mut read_buf = Cursor::new(bytes);
            let mut write_buf = Cursor::new(vec![]);

            encrypt(
              &HashSet::from_iter(vec![keys]),
              &mut read_buf,
              &mut write_buf,
              0,
              None,
            )
            .unwrap();

            let data = write_buf.into_inner();

            let data = match (start, end) {
              (None, None) => data,
              (Some(start), None) => data[start as usize..].to_vec(),
              (None, Some(end)) => data[..end as usize].to_vec(),
              (Some(start), Some(end)) => data[start as usize..end as usize].to_vec(),
            };

            let stream = ReaderStream::new(Cursor::new(data));
            let body = StreamBody::new(stream);

            return (StatusCode::OK, body).into_response();
          }

          let mut bytes = vec![];
          let path = default_dir().join("data/bam/htsnexus_test_NA12878.bam");
          File::open(path)
            .await
            .unwrap()
            .read_to_end(&mut bytes)
            .await
            .unwrap();

          let bytes = bytes[..4668].to_vec();

          let stream = ReaderStream::new(Cursor::new(bytes));
          let body = StreamBody::new(stream);

          (StatusCode::OK, body).into_response()
        },
      ),
    )
    .route(
      "/endpoint_index/:id",
      get(|AxumPath(id): AxumPath<String>| async move {
        let entry = WalkDir::new(default_dir().join("data"))
          .min_depth(2)
          .into_iter()
          .filter_entry(|e| {
            e.path()
              .file_name()
              .map(|file| {
                file.to_string_lossy() == id.clone() && !file.to_string_lossy().ends_with(".gzi")
              })
              .unwrap_or(false)
          })
          .filter_map(|v| v.ok())
          .next();

        match entry {
          None => {
            let bytes: Vec<u8> = vec![];
            let stream = ReaderStream::new(Cursor::new(bytes));
            let body = StreamBody::new(stream);

            (StatusCode::NOT_FOUND, body).into_response()
          }
          Some(entry) => {
            let mut bytes = vec![];
            let path = entry.path();
            File::open(path)
              .await
              .unwrap()
              .read_to_end(&mut bytes)
              .await
              .unwrap();

            let stream = ReaderStream::new(Cursor::new(bytes));
            let body = StreamBody::new(stream);

            (StatusCode::OK, body).into_response()
          }
        }
      }),
    )
    .route(
      "/endpoint_file/:id",
      head(
        |AxumPath(id): AxumPath<String>, headers: HeaderMap| async move {
          assert_eq!(
            headers.get(USER_AGENT),
            Some(&HeaderValue::from_static("user-agent"))
          );

          #[cfg(feature = "crypt4gh")]
          if headers.contains_key("client-public-key") {
            let public_key = read_public_key(
              general_purpose::STANDARD
                .decode(headers.get("client-public-key").unwrap())
                .unwrap(),
            )
            .await
            .unwrap()
            .into_inner();
            assert_eq!(
              public_key,
              expected_key_pair().public_key().clone().into_inner()
            );
          }

          let length = WalkDir::new(default_dir().join("data"))
            .min_depth(2)
            .into_iter()
            .filter_entry(|e| {
              e.path()
                .file_name()
                .map(|file| file.to_string_lossy() == id.clone())
                .unwrap_or(false)
            })
            .filter_map(|v| v.ok())
            .next()
            .map(|entry| entry.metadata().unwrap().len().to_string())
            .unwrap_or_else(|| "2596799".to_string());

          axum::response::Response::builder()
            .header("server-additional-bytes", 124)
            .header("client-additional-bytes", 124)
            .header(CONTENT_LENGTH, HeaderValue::from_str(&length).unwrap())
            .status(StatusCode::OK)
            .body(StreamBody::new(ReaderStream::new(Cursor::new(vec![]))))
            .unwrap()
            .into_response()
        },
      ),
    )
    .nest_service("/assets", ServeDir::new(server_base_path.to_str().unwrap()));

  #[cfg(feature = "crypt4gh")]
  let router = router.route_layer(middleware::from_fn(test_auth));

  // TODO fix this in htsget-test to bind and return tcp listener.
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let addr = listener.local_addr().unwrap();

  tokio::spawn(
    axum::Server::from_tcp(listener)
      .unwrap()
      .serve(router.into_make_service()),
  );

  test(format!("http://{}", addr)).await;
}
