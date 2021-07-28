use actix_web::{error::Error, http::StatusCode, HttpRequest, HttpResponse, Responder};
use futures_util::future::{err, ok, Ready};
use serde::Serialize;

pub struct PrettyJson<T>(pub T);

impl<T: Serialize> Responder for PrettyJson<T> {
  type Error = Error;
  type Future = Ready<Result<HttpResponse, Error>>;

  fn respond_to(self, _: &HttpRequest) -> Self::Future {
    let body = match serde_json::to_string_pretty(&self.0) {
      Ok(body) => body,
      Err(e) => return err(e.into()),
    };

    ok(
      HttpResponse::build(StatusCode::OK)
        .content_type("application/json")
        .body(body),
    )
  }
}
