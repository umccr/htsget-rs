use std::collections::HashMap;

use htsget_search::htsget::{Class, Format, Response, Url};
use serde::Serialize;

/// A helper struct to convert [Responses](Response) to JSON. It implements [serde's Serialize trait](Serialize),
/// so it's trivial to convert to JSON.
#[derive(Debug, PartialEq, Serialize)]
pub struct JsonResponse {
  format: String,
  urls: Vec<JsonUrl>,
}

impl JsonResponse {
  /// Converts a [Response] to JSON
  pub fn from_response(response: Response) -> Self {
    let format = match response.format {
      Format::Unsupported(_) => panic!("Response with an unsupported format"),
      format => format.to_string(),
    };
    let urls = response.urls.into_iter().map(JsonUrl::new).collect();
    JsonResponse { format, urls }
  }
}

/// A helper struct to convert [Urls](Url) to JSON. It shouldn't be used
/// on its own, but with [JsonResponse]
#[derive(Debug, PartialEq, Serialize)]
pub struct JsonUrl {
  url: String,
  headers: HashMap<String, String>,
  class: String,
}

impl JsonUrl {
  fn new(url: Url) -> Self {
    JsonUrl {
      url: url.url,
      headers: match url.headers {
        Some(headers) => headers.get_inner(),
        None => HashMap::new(),
      },
      class: match url.class {
        Class::Body => "body",
        Class::Header => "header",
      }
      .to_string(),
    }
  }
}
