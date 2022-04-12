use actix_web::{http::StatusCode, HttpRequest, HttpResponse, Responder};
use actix_web::body::BoxBody;
use serde::Serialize;

pub struct PrettyJson<T>(pub T);

impl<T: Serialize> Responder for PrettyJson<T> {
  type Body = BoxBody;

  fn respond_to(self, _: &HttpRequest) -> HttpResponse {
    let mut body = match serde_json::to_string_pretty(&self.0) {
      Ok(body) => body,
      Err(e) => return HttpResponse::from_error(e),
    };
    body.push('\n');

    HttpResponse::build(StatusCode::OK)
      .content_type("application/json")
      .body(body)
  }
}
