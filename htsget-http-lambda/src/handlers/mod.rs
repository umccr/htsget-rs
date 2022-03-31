
use lambda_http::http::{header, StatusCode};
use lambda_http::IntoResponse;
use serde::Serialize;
use crate::{Body, Response};

pub struct FormatJson<T>(pub T);

impl<T: Serialize> IntoResponse for FormatJson<T> {
  fn into_response(self) -> Response<Body> {
    let mut body = match serde_json::to_string_pretty(&self.0) {
      Ok(body) => body,
      Err(e) => return from_error(e),
    };
    body.push('\n');

    Response::builder().status(StatusCode::OK).header(header::CONTENT_TYPE, "application/json").body(body).expect("Expected valid response.").into_response()
  }
}

fn from_error(error: serde_json::Error) -> Response<Body> {
  unimplemented!()
}