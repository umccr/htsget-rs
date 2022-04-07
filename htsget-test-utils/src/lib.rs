pub mod server_tests;

use std::collections::HashMap;
use std::num::NonZeroU16;
use std::path::Path;
use htsget_http_core::JsonResponse;
use htsget_search::htsget::{Format, Headers, Url};
use htsget_search::htsget::Response as HtsgetResponse;
use async_trait::async_trait;
use http::{Method, StatusCode};

pub struct Header<T: Into<String>> {
  name: T,
  value: T
}

pub struct Response {
  status: u16,
  body: JsonResponse
}

#[async_trait]
pub trait TestServer {
  fn insert_header(self, header: Header<impl Into<String>>) -> Self;
  fn set_payload(self, payload: impl Into<String>) -> Self;
  fn uri(self, uri: impl Into<String>) -> Self;
  fn method(self, method: impl Into<String>) -> Self;
  async fn test_server(self) -> Response;
}