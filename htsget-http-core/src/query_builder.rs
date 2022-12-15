use tracing::instrument;

use htsget_search::htsget::{Class, Fields, Format, Query, Tags};

use crate::error::{HtsGetError, Result};

/// A helper struct to construct a [Query] from [Strings](String)
#[derive(Debug)]
pub struct QueryBuilder {
  query: Query,
}

impl QueryBuilder {
  pub fn new(id: Option<impl Into<String>>, format: Option<impl Into<String>>) -> Result<Self> {
    let format = format
      .map(Into::into)
      .ok_or_else(|| HtsGetError::InvalidInput("format required".to_string()))?;

    Ok(Self {
      query: Query::new(
        id.ok_or_else(|| HtsGetError::InvalidInput("ID required".to_string()))?,
        match format.as_str() {
          "BAM" => Format::Bam,
          "CRAM" => Format::Cram,
          "VCF" => Format::Vcf,
          "BCF" => Format::Bcf,
          _ => {
            return Err(HtsGetError::UnsupportedFormat(format!(
              "`{format}` isn't supported"
            )))
          }
        },
      ),
    })
  }

  pub fn build(self) -> Query {
    self.query
  }

  #[instrument(level = "trace", skip_all, ret)]
  pub fn with_class(mut self, class: Option<impl Into<String>>) -> Result<Self> {
    let class = class.map(Into::into);
    self.query = self.query.with_class(match class {
      None => Class::Body,
      Some(class) if class == "header" => Class::Header,
      Some(class) => {
        return Err(HtsGetError::InvalidInput(format!(
          "invalid class `{class}`"
        )))
      }
    });

    Ok(self)
  }

  #[instrument(level = "trace", skip_all, ret)]
  pub fn with_reference_name(mut self, reference_name: Option<impl Into<String>>) -> Self {
    if let Some(reference_name) = reference_name {
      self.query = self.query.with_reference_name(reference_name);
    }
    self
  }

  #[instrument(level = "trace", skip_all, ret)]
  pub fn with_range(
    self,
    start: Option<impl Into<String>>,
    end: Option<impl Into<String>>,
  ) -> Result<Self> {
    let start = start
      .map(Into::into)
      .map(|start| {
        start
          .parse::<u32>()
          .map_err(|err| HtsGetError::InvalidInput(format!("`{start}` isn't a valid start: {err}")))
      })
      .transpose()?;
    let end = end
      .map(Into::into)
      .map(|end| {
        end
          .parse::<u32>()
          .map_err(|err| HtsGetError::InvalidInput(format!("`{end}` isn't a valid end: {err}")))
      })
      .transpose()?;

    self.with_range_from_u32(start, end)
  }

  pub fn with_range_from_u32(mut self, start: Option<u32>, end: Option<u32>) -> Result<Self> {
    if let Some(start) = start {
      self.query = self.query.with_start(start);
    }
    if let Some(end) = end {
      self.query = self.query.with_end(end);
    }

    if (self.query.interval.start.is_some() || self.query.interval.end.is_some())
      && self
        .query
        .reference_name
        .as_ref()
        .filter(|name| *name != "*")
        .is_none()
    {
      return Err(HtsGetError::InvalidInput(
        "reference name must be specified with start or end range".to_string(),
      ));
    }

    if let (Some(start), Some(end)) = &(self.query.interval.start, self.query.interval.end) {
      if start > end {
        return Err(HtsGetError::InvalidRange(format!(
          "end is greater than start (`{start}` > `{end}`)"
        )));
      }
    }

    Ok(self)
  }

  #[instrument(level = "trace", skip_all, ret)]
  pub fn with_fields(self, fields: Option<impl Into<String>>) -> Self {
    self.with_fields_from_vec(
      fields.map(|fields| fields.into().split(',').map(|s| s.to_string()).collect()),
    )
  }

  pub fn with_fields_from_vec(mut self, fields: Option<Vec<impl Into<String>>>) -> Self {
    if let Some(fields) = fields {
      self.query = self
        .query
        .with_fields(Fields::List(fields.into_iter().map(Into::into).collect()));
    }

    self
  }

  #[instrument(level = "trace", skip_all, ret)]
  pub fn with_tags(
    self,
    tags: Option<impl Into<String>>,
    notags: Option<impl Into<String>>,
  ) -> Result<Self> {
    self.with_tags_from_vec(
      tags.map(|tags| tags.into().split(',').map(|s| s.to_string()).collect()),
      notags.map(|notags| notags.into().split(',').map(|s| s.to_string()).collect()),
    )
  }

  pub fn with_tags_from_vec(
    mut self,
    tags: Option<Vec<impl Into<String>>>,
    notags: Option<Vec<impl Into<String>>>,
  ) -> Result<Self> {
    let notags = match notags {
      Some(notags) => notags.into_iter().map(Into::into).collect(),
      None => vec![],
    };

    if let Some(tags) = tags {
      let tags: Vec<String> = tags.into_iter().map(Into::into).collect();
      if tags.iter().any(|tag| notags.contains(tag)) {
        return Err(HtsGetError::InvalidInput(
          "tags and notags can't intersect".to_string(),
        ));
      }
      self.query = self.query.with_tags(Tags::List(tags));
    };

    if !notags.is_empty() {
      self.query = self.query.with_no_tags(notags);
    }

    Ok(self)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn query_without_id() {
    let option: Option<&str> = None;
    assert!(matches!(
      QueryBuilder::new(option, Some("BAM")).unwrap_err(),
      HtsGetError::InvalidInput(_)
    ));
  }

  #[test]
  fn query_with_id() {
    assert_eq!(
      QueryBuilder::new(Some("ValidId".to_string()), Some("BAM"))
        .unwrap()
        .build()
        .id,
      "ValidId".to_string()
    );
  }

  #[test]
  fn query_with_format() {
    assert_eq!(
      QueryBuilder::new(Some("ValidID"), Some("VCF"))
        .unwrap()
        .build()
        .format,
      Format::Vcf
    );
  }

  #[test]
  fn query_with_invalid_format() {
    assert!(matches!(
      QueryBuilder::new(Some("ValidID"), Some("Invalid")).unwrap_err(),
      HtsGetError::UnsupportedFormat(_)
    ));
  }

  #[test]
  fn query_with_class() {
    assert_eq!(
      QueryBuilder::new(Some("ValidID"), Some("BAM"))
        .unwrap()
        .with_class(Some("header"))
        .unwrap()
        .build()
        .class,
      Class::Header
    );
  }

  #[test]
  fn query_with_reference_name() {
    assert_eq!(
      QueryBuilder::new(Some("ValidID"), Some("BAM"))
        .unwrap()
        .with_reference_name(Some("ValidName"))
        .build()
        .reference_name,
      Some("ValidName".to_string())
    );
  }

  #[test]
  fn query_with_range() {
    let query = QueryBuilder::new(Some("ValidID"), Some("BAM"))
      .unwrap()
      .with_reference_name(Some("ValidName"))
      .with_range(Some("3"), Some("5"))
      .unwrap()
      .build();
    assert_eq!(
      (query.interval.start, query.interval.end),
      (Some(3), Some(5))
    );
  }

  #[test]
  fn query_with_range_but_without_reference_name() {
    assert!(matches!(
      QueryBuilder::new(Some("ValidID"), Some("BAM"))
        .unwrap()
        .with_range(Some("3"), Some("5"))
        .unwrap_err(),
      HtsGetError::InvalidInput(_)
    ));
  }

  #[test]
  fn query_with_invalid_start() {
    assert!(matches!(
      QueryBuilder::new(Some("ValidID"), Some("BAM"))
        .unwrap()
        .with_reference_name(Some("ValidName"))
        .with_range(Some("a"), Some("5"))
        .unwrap_err(),
      HtsGetError::InvalidInput(_)
    ));
  }

  #[test]
  fn query_with_invalid_end() {
    assert!(matches!(
      QueryBuilder::new(Some("ValidID"), Some("BAM"))
        .unwrap()
        .with_reference_name(Some("ValidName"))
        .with_range(Some("5"), Some("a"))
        .unwrap_err(),
      HtsGetError::InvalidInput(_)
    ));
  }

  #[test]
  fn query_with_invalid_range() {
    assert!(matches!(
      QueryBuilder::new(Some("ValidID"), Some("BAM"))
        .unwrap()
        .with_reference_name(Some("ValidName"))
        .with_range(Some("5"), Some("3"))
        .unwrap_err(),
      HtsGetError::InvalidRange(_)
    ));
  }

  #[test]
  fn query_with_fields() {
    assert_eq!(
      QueryBuilder::new(Some("ValidID"), Some("BAM"))
        .unwrap()
        .with_fields(Some("header,part1,part2"))
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
    let query = QueryBuilder::new(Some("ValidID"), Some("BAM"))
      .unwrap()
      .with_tags(Some("header,part1,part2"), Some("part3"))
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
    let query = QueryBuilder::new(Some("ValidID"), Some("BAM"))
      .unwrap()
      .with_tags(Some("header,part1,part2"), Some("part3"))
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
