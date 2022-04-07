pub mod server_tests;

use std::collections::HashMap;
use std::num::NonZeroU16;
use std::path::Path;
use htsget_http_core::JsonResponse;
use htsget_search::htsget::{Format, Headers, Url};
use htsget_search::htsget::Response as HtsgetResponse;
use async_trait::async_trait;
use bytes::Bytes;
use http::{Method, StatusCode};
use serde::{de, Deserializer};

pub struct Header<T: Into<String>> {
  name: T,
  value: T
}

pub struct Response {
  status: u16,
  body: Bytes
}

impl Response {
  pub fn deserialize_body<'a, T>(&'a self) -> Result<T, serde_json::Error> where
    T: de::Deserialize<'a> {
    serde_json::from_slice(&self.body)
  }

  pub fn is_success(&self) -> bool {
    300 > self.status && self.status >= 200
  }
}

pub trait TestRequest {
  fn insert_header(self, header: Header<impl Into<String>>) -> Self;
  fn set_payload(self, payload: impl Into<String>) -> Self;
  fn uri(self, uri: impl Into<String>) -> Self;
  fn method(self, method: impl Into<String>) -> Self;
}

#[async_trait]
pub trait TestServer<T: TestRequest> {
  fn get_request(&self) -> T;
  async fn test_server(&self, request: impl TestRequest) -> Response;
}