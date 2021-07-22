use std::collections::HashMap;

use htsget_search::htsget::{Class, Format, Response, Url};
use serde::Serialize;

/// A helper struct to convert [Responses](Response) to JSON. It shouldn't be used
/// on its own, but with the `from_response` associated function
#[derive(Serialize)]
pub struct JsonResponse {
  format: String,
  urls: Vec<JsonUrl>,
}

impl JsonResponse {
  /// Converts a [Response] to JSON
  pub fn from_response(response: Response) -> String {
    let format = match response.format {
      Format::Unsupported(_) => panic!("Response with an unsupported format"),
      format => format.to_string(),
    };
    let urls = response.urls.into_iter().map(JsonUrl::new).collect();
    serde_json::to_string_pretty(&JsonResponse { format, urls })
      .expect("Internal error while converting response to json")
  }
}

/// A helper struct to convert [Urls](Url) to JSON. It shouldn't be used
/// on its own, but with [JsonResponse]
#[derive(Serialize)]
struct JsonUrl {
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
