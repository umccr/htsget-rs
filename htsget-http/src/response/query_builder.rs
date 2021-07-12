use crate::error::{HtsGetError, Result};
use htsget_search::htsget::{Class, Fields, Format, Query, Tags};

#[derive(Debug)]
pub struct QueryBuilder {
  query: Query,
}

impl QueryBuilder {
  pub fn new(id: Option<impl Into<String>>) -> Result<Self> {
    Ok(QueryBuilder {
      query: Query::new(id.ok_or_else(|| HtsGetError::InvalidInput("ID needed".to_string()))?),
    })
  }

  pub fn build(self) -> Query {
    self.query
  }

  pub fn add_format(mut self, format: Option<impl Into<String>>) -> Result<Self> {
    let format = format.map(|s| s.into());
    if let Some(format) = format {
      self.query = self.query.with_format(match format.as_str() {
        "BAM" => Format::Bam,
        "CRAM" => Format::Cram,
        "VCF" => Format::Vcf,
        "BCF" => Format::Bcf,
        _ => {
          return Err(HtsGetError::UnsupportedFormat(format!(
            "The {} format isn't supported",
            format
          )))
        }
      })
    }
    Ok(self)
  }

  pub fn add_class(mut self, class: Option<impl Into<String>>) -> Result<Self> {
    let class = class.map(|s| s.into());
    self.query = self.query.with_class(match class {
      None => Class::Body,
      Some(class) if class == "header" => Class::Header,
      Some(class) => {
        return Err(HtsGetError::InvalidInput(format!(
          "Invalid class: {}",
          class
        )))
      }
    });
    Ok(self)
  }

  pub fn add_reference_name(mut self, reference_name: Option<impl Into<String>>) -> Self {
    if let Some(reference_name) = reference_name {
      self.query = self.query.with_reference_name(reference_name);
    }
    self
  }

  pub fn add_range(
    mut self,
    start: Option<impl Into<String>>,
    end: Option<impl Into<String>>,
  ) -> Result<Self> {
    let start = start.map(|s| s.into());
    let end = end.map(|s| s.into());
    if let Some(start) = start {
      self.query = self.query.with_start(
        start
          .parse::<u32>()
          .map_err(|_| HtsGetError::InvalidInput(format!("{} isn't a valid start", start)))?,
      );
    }
    if let Some(end) = end {
      self.query = self.query.with_end(
        end
          .parse::<u32>()
          .map_err(|_| HtsGetError::InvalidInput(format!("{} isn't a valid end", end)))?,
      );
    }
    if (self.query.start.is_some() || self.query.end.is_some())
      && (self.query.reference_name.is_none() || self.query.reference_name.clone().unwrap() == "*")
    {
      return Err(HtsGetError::InvalidInput(
        "Can't use range whitout specifying the reference name or with \"*\"".to_string(),
      ));
    }
    if self.query.start.is_some()
      && self.query.end.is_some()
      && self.query.start.unwrap() > self.query.end.unwrap()
    {
      return Err(HtsGetError::InvalidRange(format!(
        "end({}) is greater than start({})",
        self.query.end.unwrap(),
        self.query.start.unwrap()
      )));
    }
    Ok(self)
  }

  pub fn add_fields(mut self, fields: Option<impl Into<String>>) -> Self {
    if let Some(fields) = fields {
      self.query = self.query.with_fields(Fields::List(
        fields.into().split(',').map(|s| s.to_string()).collect(),
      ));
    }
    self
  }

  pub fn add_tags(
    mut self,
    tags: Option<impl Into<String>>,
    notags: Option<impl Into<String>>,
  ) -> Result<Self> {
    let notags = match notags {
      Some(notags) => notags.into().split(',').map(|s| s.to_string()).collect(),
      None => vec![],
    };
    if let Some(tags) = tags {
      let tags: Vec<String> = tags.into().split(',').map(|s| s.to_string()).collect();
      if tags.iter().any(|tag| notags.contains(tag)) {
        return Err(HtsGetError::InvalidInput(
          "Tags and notags can't intersect".to_string(),
        ));
      }
      self.query = self.query.with_tags(Tags::List(tags));
    };
    self.query = self.query.with_no_tags(notags);
    Ok(self)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn query_without_id() {
    let option: Option<&str> = None;
    assert_eq!(
      QueryBuilder::new(option).unwrap_err(),
      HtsGetError::InvalidInput("ID needed".to_string())
    );
  }

  #[test]
  fn query_with_id() {
    assert_eq!(
      QueryBuilder::new(Some("ValidId".to_string()))
        .unwrap()
        .build()
        .id,
      "ValidId".to_string()
    )
  }

  #[test]
  fn query_with_format() {
    assert_eq!(
      QueryBuilder::new(Some("ValidID"))
        .unwrap()
        .add_format(Some("VCF"))
        .unwrap()
        .build()
        .format
        .unwrap(),
      Format::Vcf
    );
  }

  #[test]
  fn query_with_invalid_format() {
    assert_eq!(
      QueryBuilder::new(Some("ValidID"))
        .unwrap()
        .add_format(Some("Invalid"))
        .unwrap_err(),
      HtsGetError::UnsupportedFormat(format!("The Invalid format isn't supported"))
    );
  }

  #[test]
  fn query_with_class() {
    assert_eq!(
      QueryBuilder::new(Some("ValidID"))
        .unwrap()
        .add_class(Some("header"))
        .unwrap()
        .build()
        .class,
      Class::Header
    );
  }

  #[test]
  fn query_with_reference_name() {
    assert_eq!(
      QueryBuilder::new(Some("ValidID"))
        .unwrap()
        .add_reference_name(Some("ValidName"))
        .build()
        .reference_name,
      Some("ValidName".to_string())
    );
  }

  #[test]
  fn query_with_range() {
    let query = QueryBuilder::new(Some("ValidID"))
      .unwrap()
      .add_reference_name(Some("ValidName"))
      .add_range(Some("3"), Some("5"))
      .unwrap()
      .build();
    assert_eq!((query.start, query.end), (Some(3), Some(5)));
  }

  #[test]
  fn query_with_range_but_without_reference_name() {
    assert_eq!(
      QueryBuilder::new(Some("ValidID"))
        .unwrap()
        .add_range(Some("3"), Some("5"))
        .unwrap_err(),
      HtsGetError::InvalidInput(format!(
        "Can't use range whitout specifying the reference name or with \"*\""
      ))
    );
  }

  #[test]
  fn query_with_invalid_start() {
    assert_eq!(
      QueryBuilder::new(Some("ValidID"))
        .unwrap()
        .add_reference_name(Some("ValidName"))
        .add_range(Some("a"), Some("5"))
        .unwrap_err(),
      HtsGetError::InvalidInput(format!("a isn't a valid start"))
    );
  }

  #[test]
  fn query_with_invalid_range() {
    assert_eq!(
      QueryBuilder::new(Some("ValidID"))
        .unwrap()
        .add_reference_name(Some("ValidName"))
        .add_range(Some("5"), Some("3"))
        .unwrap_err(),
      HtsGetError::InvalidRange(format!("end(3) is greater than start(5)"))
    );
  }

  #[test]
  fn query_with_fields() {
    assert_eq!(
      QueryBuilder::new(Some("ValidID"))
        .unwrap()
        .add_fields(Some("header,part1,part2"))
        .build()
        .fields,
      Fields::List(vec![
        "header".to_string(),
        "part1".to_string(),
        "part2".to_string()
      ])
    );
  }

  #[test]
  fn query_with_tags() {
    let query = QueryBuilder::new(Some("ValidID"))
      .unwrap()
      .add_tags(Some("header,part1,part2"), Some("part3"))
      .unwrap()
      .build();
    assert_eq!(
      query.tags,
      Tags::List(vec![
        "header".to_string(),
        "part1".to_string(),
        "part2".to_string()
      ])
    );
    assert_eq!(query.no_tags, Some(vec!["part3".to_string()]));
  }

  #[test]
  fn query_with_invalid_tags() {
    let query = QueryBuilder::new(Some("ValidID"))
      .unwrap()
      .add_tags(Some("header,part1,part2"), Some("part3"))
      .unwrap()
      .build();
    assert_eq!(
      query.tags,
      Tags::List(vec![
        "header".to_string(),
        "part1".to_string(),
        "part2".to_string()
      ])
    );
    assert_eq!(query.no_tags, Some(vec!["part3".to_string()]));
  }
}
