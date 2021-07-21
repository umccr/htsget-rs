mod query_builder;
use query_builder::QueryBuilder;
mod error;
pub use error::{HtsGetError, Result};
mod json_response;
use json_response::JsonResponse;
mod post_request;
pub use post_request::{PostRequest, Region};

use htsget_search::htsget::{HtsGet, Query, Response};
use std::collections::HashMap;

const READS_DEFAULT_FORMAT: &str = "BAM";
const VARIANTS_DEFAULT_FORMAT: &str = "VCF";
const READS_FORMATS: [&str; 2] = ["BAM", "CRAM"];
const VARIANTS_FORMATS: [&str; 2] = ["VCF", "BCF"];

pub enum Endpoint {
  Reads,
  Variants,
}

/// Gets a JSON response for a GET request. The GET request parameters must
/// be in a HashMap. The "id" field is the only mandatory one. The rest can be
/// consulted [here](https://samtools.github.io/hts-specs/htsget.html)
pub fn get_response_for_get_request<H: HtsGet>(
  searcher: &H,
  mut query_information: HashMap<String, String>,
  endpoint: Endpoint,
) -> Result<String> {
  match (endpoint, query_information.get(&"format".to_string())) {
    (Endpoint::Reads, None) => {
      query_information.insert("format".to_string(), READS_DEFAULT_FORMAT.to_string());
    }
    (Endpoint::Variants, None) => {
      query_information.insert("format".to_string(), VARIANTS_DEFAULT_FORMAT.to_string());
    }
    (Endpoint::Reads, Some(s)) if READS_FORMATS.contains(&s.as_str()) => (),
    (Endpoint::Variants, Some(s)) if VARIANTS_FORMATS.contains(&s.as_str()) => (),
    (_, Some(s)) => {
      return Err(HtsGetError::UnsupportedFormat(format!(
        "{} isn't a supported format",
        s
      )))
    }
  }
  let query = convert_to_query(&query_information)?;
  searcher
    .search(query)
    .map_err(|error| error.into())
    .map(JsonResponse::from_response)
}

fn convert_to_query(query_information: &HashMap<String, String>) -> Result<Query> {
  Ok(
    QueryBuilder::new(query_information.get("id"))?
      .with_format(query_information.get("format"))?
      .with_class(query_information.get("class"))?
      .with_reference_name(query_information.get("referenceName"))
      .with_range(query_information.get("start"), query_information.get("end"))?
      .with_fields(query_information.get("fields"))
      .with_tags(
        query_information.get("tags"),
        query_information.get("notags"),
      )?
      .build(),
  )
}

/// Gets a response in JSON for a POST request.
/// The parameters can be consulted [here](https://samtools.github.io/hts-specs/htsget.html)
pub fn get_response_for_post_request<H: HtsGet>(
  searcher: &H,
  mut request: PostRequest,
  id: impl Into<String>,
  endpoint: Endpoint,
) -> Result<String> {
  match (endpoint, &request.format) {
    (Endpoint::Reads, None) => request.format = Some(READS_DEFAULT_FORMAT.to_string()),
    (Endpoint::Variants, None) => request.format = Some(VARIANTS_DEFAULT_FORMAT.to_string()),
    (Endpoint::Reads, Some(s)) if READS_FORMATS.contains(&s.as_str()) => (),
    (Endpoint::Variants, Some(s)) if VARIANTS_FORMATS.contains(&s.as_str()) => (),
    (_, Some(s)) => {
      return Err(HtsGetError::UnsupportedFormat(format!(
        "{} isn't a supported format",
        s
      )))
    }
  }
  let responses = request
    .get_queries(id)?
    .into_iter()
    .map(|query| searcher.search(query).map_err(|error| error.into()))
    .collect::<Result<Vec<Response>>>()?;
  Ok(JsonResponse::from_response(
    // It's okay to unwrap because there will be at least one response
    merge_responses(responses).unwrap(),
  ))
}

fn merge_responses(responses: Vec<Response>) -> Option<Response> {
  responses.into_iter().reduce(|mut acc, mut response| {
    acc.urls.append(&mut response.urls);
    acc
  })
}
