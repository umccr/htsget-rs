use crate::http::{Header, TestRequest, TestServer};
use http::Method;
use http::header::{
  ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
  ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD, ORIGIN,
};

/// A simple cors request test.
pub async fn test_cors_simple_request<T: TestRequest>(tester: &impl TestServer<T>) {
  test_cors_simple_request_uri(tester, "/variants/service-info").await;
}

/// A simple cors request test, with uri specified.
pub async fn test_cors_simple_request_uri<T: TestRequest>(tester: &impl TestServer<T>, uri: &str) {
  let request = tester
    .request()
    .method(Method::GET)
    .uri(uri)
    .insert_header(Header {
      name: ORIGIN,
      value: http::HeaderValue::from_static("http://example.com"),
    });
  let response = tester
    .test_server(request, tester.get_expected_path().await)
    .await;

  assert!(response.is_success());
  assert_eq!(
    response
      .headers
      .get(ACCESS_CONTROL_ALLOW_ORIGIN)
      .unwrap()
      .to_str()
      .unwrap(),
    "http://example.com"
  );
}

/// A preflight cors request test.
pub async fn test_cors_preflight_request<T: TestRequest>(
  tester: &impl TestServer<T>,
  expected_headers: &str,
  expected_methods_contains: &str,
) {
  test_cors_preflight_request_uri(
    tester,
    "/variants/service-info",
    expected_headers,
    expected_methods_contains,
  )
  .await;
}

/// A preflight cors request test, with uri specified.
pub async fn test_cors_preflight_request_uri<T: TestRequest>(
  tester: &impl TestServer<T>,
  uri: &str,
  expected_headers: &str,
  expected_methods_contains: &str,
) {
  let request = tester
    .request()
    .method(Method::OPTIONS)
    .uri(uri)
    .insert_header(Header {
      name: ORIGIN,
      value: http::HeaderValue::from_static("http://example.com"),
    })
    .insert_header(Header {
      name: ACCESS_CONTROL_REQUEST_HEADERS,
      value: http::HeaderValue::from_static("X-Requested-With"),
    })
    .insert_header(Header {
      name: ACCESS_CONTROL_REQUEST_METHOD,
      value: http::HeaderValue::from_static("POST"),
    });
  let response = tester
    .test_server(request, tester.get_expected_path().await)
    .await;

  assert!(response.is_success());
  assert_eq!(
    response
      .headers
      .get(ACCESS_CONTROL_ALLOW_ORIGIN)
      .unwrap()
      .to_str()
      .unwrap()
      .to_lowercase(),
    "http://example.com"
  );

  assert_eq!(
    response
      .headers
      .get(ACCESS_CONTROL_ALLOW_HEADERS)
      .unwrap()
      .to_str()
      .unwrap()
      .to_lowercase(),
    expected_headers
  );

  assert!(
    response
      .headers
      .get(ACCESS_CONTROL_ALLOW_METHODS)
      .unwrap()
      .to_str()
      .unwrap()
      .contains(expected_methods_contains)
  );
}
