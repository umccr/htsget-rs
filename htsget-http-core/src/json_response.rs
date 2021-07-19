use std::collections::HashMap;

use htsget_search::htsget::{Class, Format, Response, Url};
use serde::Serialize;

#[derive(Serialize)]
pub struct JsonResponse {
  format: String,
  urls: Vec<JsonUrl>,
}

impl JsonResponse {
  pub fn new(response: Response) -> String {
    // TODO: Use .to_string() when https://github.com/umccr/htsget-rs/pull/52 is merged
    let format = match response.format {
      Format::Bam => "BAM",
      Format::Cram => "CRAM",
      Format::Vcf => "VCF",
      Format::Bcf => "BCF",
      Format::Unsupported(_) => panic!("Response with an unsupported format"),
    }
    .to_string();
    let urls = response
      .urls
      .into_iter()
      .map(|url| JsonUrl::new(url))
      .collect();
    serde_json::to_string_pretty(&JsonResponse { format, urls })
      .expect("Internal error while converting response to json")
  }
}

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
