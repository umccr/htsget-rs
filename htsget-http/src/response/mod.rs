use crate::error::{HtsGetError, Result};
use htsget_search::htsget::{Class, Format, Query, Response};
use std::collections::HashMap;

pub fn get_response(queryInformation: HashMap<String, String>) -> Result<Response> {
  Err(HtsGetError::InvalidRange("No".to_string()))
}

fn convert_to_query(queryInformation: HashMap<String, String>) -> Result<Query> {
  let query = Query::new(
    queryInformation
      .get("id")
      .ok_or_else(|| HtsGetError::InvalidInput("ID needed".to_string()))?
      .clone(),
  );
  let query = add_format(query, queryInformation.get("format"))?;
  let query = add_class(query, queryInformation.get("class"))?;
  let query = add_reference_name(query, queryInformation.get("reference_name"));
  let query = add_range(
    query,
    queryInformation.get("start"),
    queryInformation.get("end"),
  )?;
  let query = add_fields(query, queryInformation.get("fields"));
  let query = add_fields(query, queryInformation.get("fields"));
  let query = add_tags(
    query,
    queryInformation.get("tags"),
    queryInformation.get("notags"),
  )?;
  Ok(query)
}

fn add_format(query: Query, format: Option<&String>) -> Result<Query> {
  if let Some(format) = format {
    Ok(query.with_format(match format.as_str() {
      "BAM" => Format::Bam,
      "CRAM" => Format::Bam,
      "VCF" => Format::Vcf,
      "BCF" => Format::Bcf,
      _ => {
        return Err(HtsGetError::UnsupportedFormat(format!(
          "The {} format isn't supported",
          format
        )))
      }
    }))
  } else {
    Ok(query)
  }
}

fn add_class(query: Query, class: Option<&String>) -> Result<Query> {
  Ok(query.with_class(match class {
    None => Class::Body,
    Some(class) if class == "header" => Class::Header,
    Some(class) => {
      return Err(HtsGetError::InvalidInput(format!(
        "Invalid class: {}",
        class
      )))
    }
  }))
}

fn add_reference_name(query: Query, reference_name: Option<&String>) -> Query {
  if let Some(reference_name) = reference_name {
    query.with_reference_name(reference_name)
  } else {
    query
  }
}

fn add_range(mut query: Query, start: Option<&String>, end: Option<&String>) -> Result<Query> {
  if let Some(start) = start {
    query = query.with_start(
      start
        .parse::<u32>()
        .map_err(|_| HtsGetError::InvalidInput(format!("{} isn't a valid start", start)))?,
    );
  }
  if let Some(end) = end {
    query = query.with_end(
      end
        .parse::<u32>()
        .map_err(|_| HtsGetError::InvalidInput(format!("{} isn't a valid end", end)))?,
    );
  }
  if (start.is_some() || end.is_some())
    && (query.reference_name.is_none() || query.reference_name.clone().unwrap() == "*")
  {
    return Err(HtsGetError::InvalidInput(format!(
      "Can't use range whitout specifying the reference name or with \"*\"",
    )));
  }
  if start.is_some() && end.is_some() && query.start.unwrap() < query.end.unwrap() {
    return Err(HtsGetError::InvalidRange(format!(
      "end({}) is greater than start({})",
      end.unwrap(),
      start.unwrap()
    )));
  }
  Ok(query)
}

fn add_fields(query: Query, fields: Option<&String>) -> Query {
  if let Some(fields) = fields {
    query.with_fields(fields.split(",").collect())
  } else {
    query.with_fields(vec!["all"])
  }
}

fn add_tags(query: Query, tags: Option<&String>, notags: Option<&String>) -> Result<Query> {
  let tags = match tags {
    Some(tags) => tags.split(",").collect(),
    None => vec![],
  };
  let notags = match notags {
    Some(notags) => notags.split(",").collect(),
    None => vec![],
  };
  if tags.iter().any(|tag| notags.contains(tag)) {
    return Err(HtsGetError::InvalidInput(
      "Tags and notags can't intersect".to_string(),
    ));
  }
  Ok(query)
}
