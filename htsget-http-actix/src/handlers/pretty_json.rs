use actix_web::{error::Error, http::StatusCode, HttpRequest, HttpResponse, Responder, HttpResponseBuilder};
use futures_util::future::{err, ok, Ready};
use serde::Serialize;

pub struct PrettyJson<T>(pub T);

impl<T: Serialize> Responder for PrettyJson<T> {
  //type Error = Error;
  //type Future = Ready<Result<HttpResponse, Error>>;

  fn respond_to(self, _: &HttpRequest) -> HttpResponse {
    let mut body = serde_json::to_string_pretty(&self.0).unwrap();
    body.push('\n');

    HttpResponse::Ok()
      .content_type("application/json")
        .body(body)
  }
}
