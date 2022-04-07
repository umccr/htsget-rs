use std::collections::HashMap;
use std::path::Path;
use htsget_http_core::JsonResponse;
use htsget_search::htsget::{Format, Headers, Response, Url};

fn example_response(path: &Path, headers: Option<Headers>) -> JsonResponse {
  let url = Url::new(format!(
    "file://{}",
    path
      .join("data")
      .join("vcf")
      .join("sample1-bcbio-cancer.vcf.gz")
      .to_string_lossy()
  ));
  if let Some(headers) = headers {
    JsonResponse::from_response(Response::new(
      Format::Vcf,
      vec![url.with_headers(headers)]),
    )
  } else {
    JsonResponse::from_response(Response::new(
      Format::Vcf,
      vec![url]),
    )
  }
}

fn example_headers() -> Headers {
  let mut headers = HashMap::new();
  headers.insert("Range".to_string(), "bytes=0-3367".to_string());
  Headers::new(headers)
}