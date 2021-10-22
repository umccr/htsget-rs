use lambda_http::{Request, Response, IntoResponse};
use serde::Serialize;

pub struct PrettyJson<T>(pub T);

impl<T: Serialize> IntoResponse for PrettyJson<T> {
  fn respond_to(self, _: &Request) -> Response {
    let mut body = match serde_json::to_string_pretty(&self.0) {
      Ok(body) => body,
      Err(e) => return Err(e.into()),
    };
    body.push('\n');

    Ok(
      Response::build()
        .status(200)
        .content_type("application/json")
        .body(body),
    )
  }
}
