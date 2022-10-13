use http::header::{
  ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
  ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD, ORIGIN,
};
use http::Method;

use crate::http_tests::{Header, TestRequest, TestServer};

/// A simple cors request test.
pub async fn test_cors_simple_request<T: TestRequest>(tester: &impl TestServer<T>) {
  test_cors_simple_request_uri(tester, "/variants/service-info").await;
}

/// A simple cors request test, with uri specified.
pub async fn test_cors_simple_request_uri<T: TestRequest>(tester: &impl TestServer<T>, uri: &str) {
  let request = tester
    .get_request()
    .method(Method::GET.to_string())
    .uri(uri)
    .insert_header(Header {
      name: ORIGIN.to_string(),
      value: "http://example.com".to_string(),
    });
  let response = tester.test_server(request).await;

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
pub async fn test_cors_preflight_request<T: TestRequest>(tester: &impl TestServer<T>) {
  test_cors_preflight_request_uri(tester, "/variants/service-info").await;
}

/// A preflight cors request test, with uri specified.
pub async fn test_cors_preflight_request_uri<T: TestRequest>(
  tester: &impl TestServer<T>,
  uri: &str,
) {
  let request = tester
    .get_request()
    .method(Method::OPTIONS.to_string())
    .uri(uri)
    .insert_header(Header {
      name: ORIGIN.to_string(),
      value: "http://example.com".to_string(),
    })
    .insert_header(Header {
      name: ACCESS_CONTROL_REQUEST_HEADERS.to_string(),
      value: "X-Requested-With".to_string(),
    })
    .insert_header(Header {
      name: ACCESS_CONTROL_REQUEST_METHOD.to_string(),
      value: "POST".to_string(),
    });
  let response = tester.test_server(request).await;

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

  assert_eq!(
    response
      .headers
      .get(ACCESS_CONTROL_ALLOW_HEADERS)
      .unwrap()
      .to_str()
      .unwrap(),
    "X-Requested-With"
  );

  for method in &[
    "HEAD", "GET", "OPTIONS", "PUT", "PATCH", "TRACE", "POST", "DELETE", "CONNECT",
  ] {
    assert!(response
      .headers
      .get(ACCESS_CONTROL_ALLOW_METHODS)
      .unwrap()
      .to_str()
      .unwrap()
      .contains(method));
  }
}
