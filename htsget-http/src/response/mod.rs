mod query_builder;
use query_builder::QueryBuilder;

use crate::error::{HtsGetError, Result};
use htsget_search::htsget::{Query, Response};
use std::collections::HashMap;

pub fn get_response(queryInformation: HashMap<String, String>) -> Result<Response> {
  Err(HtsGetError::InvalidRange("No".to_string()))
}

fn convert_to_query(queryInformation: HashMap<String, String>) -> Result<Query> {
  Ok(
    QueryBuilder::new(queryInformation.get("id"))?
      .add_format(queryInformation.get("format"))?
      .add_class(queryInformation.get("class"))?
      .add_reference_name(queryInformation.get("reference_name"))
      .add_range(queryInformation.get("start"), queryInformation.get("end"))?
      .add_fields(queryInformation.get("fields"))
      .add_fields(queryInformation.get("fields"))
      .add_tags(queryInformation.get("tags"), queryInformation.get("notags"))?
      .build(),
  )
}
