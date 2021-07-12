mod query_builder;
use query_builder::QueryBuilder;

use crate::error::Result;
use htsget_search::htsget::Query;
use std::collections::HashMap;

pub fn get_response(query_information: HashMap<String, String>) -> Result<String> {
  let query = convert_to_query(query_information);
  query.map(|_| "No".to_string())
}

fn convert_to_query(query_information: HashMap<String, String>) -> Result<Query> {
  Ok(
    QueryBuilder::new(query_information.get("id"))?
      .add_format(query_information.get("format"))?
      .add_class(query_information.get("class"))?
      .add_reference_name(query_information.get("reference_name"))
      .add_range(query_information.get("start"), query_information.get("end"))?
      .add_fields(query_information.get("fields"))
      .add_fields(query_information.get("fields"))
      .add_tags(
        query_information.get("tags"),
        query_information.get("notags"),
      )?
      .build(),
  )
}
