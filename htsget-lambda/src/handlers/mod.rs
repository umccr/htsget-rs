//! Module primarily providing http response functionality for the htsget endpoints.
//!

use lambda_http::http;
use lambda_http::http::{header, StatusCode};
use serde::Serialize;
use serde_json::Error;

use htsget_config::types::JsonResponse;
use htsget_http::{HtsGetError, Result};

use crate::{Body, Response};

pub mod get;
pub mod post;
pub mod service_info;

/// New type used for formatting a http response.
pub struct FormatJson<T>(T);

impl<T> FormatJson<T> {
  pub fn into_inner(self) -> T {
    self.0
  }
}

impl<T: Serialize> TryFrom<FormatJson<T>> for Response<Body> {
  type Error = http::Error;

  fn try_from(value: FormatJson<T>) -> http::Result<Self> {
    let mut body = match serde_json::to_string_pretty(&value.into_inner()) {
      Ok(body) => body,
      Err(e) => return Ok(FormatJson::try_from(e)?.into_inner()),
    };
    body.push('\n');

    Response::builder()
      .status(StatusCode::OK)
      .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
      .body(Body::from(body))
  }
}

impl TryFrom<Error> for FormatJson<Response<Body>> {
  type Error = http::Error;

  fn try_from(error: Error) -> http::Result<Self> {
    Ok(Self(
      Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header(header::CONTENT_TYPE, mime::TEXT_PLAIN_UTF_8.as_ref())
        .body(Body::from(error.to_string()))?,
    ))
  }
}

impl TryFrom<HtsGetError> for FormatJson<Response<Body>> {
  type Error = http::Error;

  fn try_from(error: HtsGetError) -> http::Result<Self> {
    let (json, status_code) = error.to_json_representation();
    let mut response: Response<Body> = FormatJson(json).try_into()?;
    *response.status_mut() = status_code;
    Ok(Self(response))
  }
}

/// Handles a response, converting errors to json and using the proper HTTP status code.
fn handle_response(response: Result<JsonResponse>) -> http::Result<Response<Body>> {
  match response {
    Err(error) => Ok(FormatJson::try_from(error)?.into_inner()),
    Ok(json) => FormatJson(json).try_into(),
  }
}

#[cfg(test)]
mod tests {
  use lambda_http::http::{header, HeaderMap, Response, StatusCode};
  use lambda_http::Body;
  use mime::Mime;
  use serde::ser::Error;
  use serde::{Serialize, Serializer};
  use serde_json::{json, Value};

  use crate::handlers::FormatJson;

  struct TestError;

  impl Serialize for TestError {
    fn serialize<S>(&self, _: S) -> Result<S::Ok, S::Error>
    where
      S: Serializer,
    {
      Err(Error::custom(json!({"value": "1"})))
    }
  }

  #[test]
  fn into_response() {
    let expected_body = json!({"value": "1"});
    let json = FormatJson(expected_body.clone());
    test_into_response(
      json.try_into().unwrap(),
      expected_body,
      StatusCode::OK,
      mime::APPLICATION_JSON,
    );
  }

  #[test]
  fn into_response_error() {
    let json = FormatJson(TestError);
    test_into_response(
      json.try_into().unwrap(),
      json!({"value": "1"}),
      StatusCode::INTERNAL_SERVER_ERROR,
      mime::TEXT_PLAIN_UTF_8,
    );
  }

  fn test_into_response(
    response: Response<Body>,
    expected_body: Value,
    expected_status_code: StatusCode,
    expected_content_type: Mime,
  ) {
    let mut expected_headers = HeaderMap::new();
    expected_headers.insert(
      header::CONTENT_TYPE,
      expected_content_type.as_ref().parse().unwrap(),
    );

    assert_eq!(response.status(), expected_status_code);
    assert_eq!(response.headers(), &expected_headers);

    let bytes: &[u8] = response.body().as_ref();
    let value: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(value, expected_body);
  }
}
