use async_trait::async_trait;
use bytes::Bytes;
use serde::de;

use htsget_config::config::HtsgetConfig;
use htsget_search::htsget::Response as HtsgetResponse;

pub mod server_tests;

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
#[derive(Debug)]
pub struct Response {
  status: u16,
  body: Bytes,
}

impl Response {
  pub fn new(status: u16, body: Bytes) -> Self {
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
  fn get_config(&self) -> &HtsgetConfig;
  fn get_request(&self) -> T;
  async fn test_server(&self, request: T) -> Response;
}
