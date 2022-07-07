use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use htsget_search::htsget::{Class, Response, Url};

/// A helper struct to convert [Responses](Response) to JSON. It implements [serde's Serialize trait](Serialize),
/// so it's trivial to convert to JSON.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonResponse {
  pub htsget: HtsGetResponse,
}

impl JsonResponse {
  /// Converts a [Response] to JSON.
  pub fn from_response(response: Response) -> Self {
    let htsget = HtsGetResponse::new(response);
    Self { htsget }
  }
}

/// A helper struct to represent a JSON response. It shouldn't be used
/// on its own, but with `JsonResponse`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct HtsGetResponse {
  pub format: String,
  pub urls: Vec<JsonUrl>,
}

impl HtsGetResponse {
  fn new(response: Response) -> Self {
    let format = response.format.to_string();
    let urls = response.urls.into_iter().map(JsonUrl::new).collect();
    Self { format, urls }
  }
}

/// A helper struct to convert [Urls](Url) to JSON. It shouldn't be used
/// on its own, but with `JsonResponse`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonUrl {
  pub url: String,
  pub headers: Option<HashMap<String, String>>,
  pub class: Option<String>,
}

impl JsonUrl {
  fn new(url: Url) -> Self {
    Self {
      url: url.url,
      headers: Some(match url.headers {
        Some(headers) => headers.get_inner(),
        None => HashMap::new(),
      }),
      class: Some(
        match url.class {
          Class::Body => "body",
          Class::Header => "header",
        }
        .to_string(),
      ),
    }
  }
}
