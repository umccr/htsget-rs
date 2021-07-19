mod query_builder;
use query_builder::QueryBuilder;
mod error;
pub use error::{HtsGetError, Result};
mod json_response;
use json_response::JsonResponse;
mod post_request;
pub use post_request::PostRequest;

use htsget_search::htsget::{HtsGet, Query, Response};
use std::collections::HashMap;

pub fn get_response_for_get_request<H: HtsGet>(
  searcher: &H,
  query_information: &HashMap<String, String>,
) -> Result<String> {
  let query = convert_to_query(query_information)?;
  searcher
    .search(query)
    .map_err(|error| error.into())
    .map(JsonResponse::from_response)
}

fn convert_to_query(query_information: &HashMap<String, String>) -> Result<Query> {
  Ok(
    QueryBuilder::new(query_information.get("id"))?
      .add_format(query_information.get("format"))?
      .add_class(query_information.get("class"))?
      .add_reference_name(query_information.get("referenceName"))
      .add_range(query_information.get("start"), query_information.get("end"))?
      .add_fields(query_information.get("fields"))
      .add_tags(
        query_information.get("tags"),
        query_information.get("notags"),
      )?
      .build(),
  )
}

pub fn get_response_for_post_request<H: HtsGet>(
  searcher: &H,
  request: PostRequest,
  id: impl Into<String>,
) -> Result<String> {
  let responses = request
    .get_queries(id)?
    .into_iter()
    .map(|query| searcher.search(query).map_err(|error| error.into()))
    .collect::<Result<Vec<Response>>>()?;
  // It's okay to unwrap because there will be at least one response
  Ok(JsonResponse::from_response(
    merge_responses(responses).unwrap(),
  ))
}

fn merge_responses(responses: Vec<Response>) -> Option<Response> {
  responses.into_iter().reduce(|mut acc, mut response| {
    acc.urls.append(&mut response.urls);
    acc
  })
}
