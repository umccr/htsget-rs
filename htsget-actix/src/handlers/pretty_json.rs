use actix_web::body::BoxBody;
use actix_web::{HttpRequest, HttpResponse, Responder, http::StatusCode};
use serde::Serialize;

pub struct PrettyJson<T>(pub T);

impl<T> TryFrom<PrettyJson<T>> for String
where
  T: Serialize,
{
  type Error = serde_json::Error;

  fn try_from(json: PrettyJson<T>) -> Result<Self, Self::Error> {
    let mut body = serde_json::to_string_pretty(&json.0)?;
    body.push('\n');

    Ok(body)
  }
}

impl<T: Serialize> Responder for PrettyJson<T> {
  type Body = BoxBody;

  fn respond_to(self, _: &HttpRequest) -> HttpResponse {
    let body = match String::try_from(self) {
      Ok(body) => body,
      Err(err) => return HttpResponse::from_error(err),
    };

    HttpResponse::build(StatusCode::OK)
      .content_type("application/json")
      .body(body)
  }
}
