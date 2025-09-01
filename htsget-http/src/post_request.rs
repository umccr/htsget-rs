use htsget_config::types::{Query, Request};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{Endpoint, QueryBuilder, Result, match_format, set_query_builder};

/// A struct to represent a POST request according to the
/// [HtsGet specification](https://samtools.github.io/hts-specs/htsget.html). It implements
/// [Deserialize] to make it more ergonomic. Each `PostRequest` can contain several regions.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct PostRequest {
  pub format: Option<String>,
  pub class: Option<String>,
  pub fields: Option<Vec<String>>,
  pub tags: Option<Vec<String>>,
  pub notags: Option<Vec<String>>,
  pub regions: Option<Vec<Region>>,
  #[serde(rename = "encryptionScheme")]
  pub encryption_scheme: Option<String>,
}

/// A struct that contains the data to quest for a specific region. It is only meant to be use
/// alongside a `PostRequest`
#[derive(Serialize, Deserialize, Debug)]
pub struct Region {
  #[serde(rename = "referenceName")]
  pub reference_name: String,
  pub start: Option<u32>,
  pub end: Option<u32>,
}

impl PostRequest {
  /// Converts the `PostRequest` into one or more equivalent [Queries](Query)
  #[instrument(level = "trace", skip_all, ret)]
  pub(crate) fn get_queries(self, request: Request, endpoint: &Endpoint) -> Result<Vec<Query>> {
    let format = match_format(endpoint, self.format.clone())?;

    if let Some(ref regions) = self.regions {
      regions
        .iter()
        .map(|region| {
          set_query_builder(
            QueryBuilder::new(request.clone(), format),
            self.class.clone(),
            Some(region.reference_name.clone()),
            Self::join_vec(self.fields.clone()),
            (
              Self::join_vec(self.tags.clone()),
              Self::join_vec(self.notags.clone()),
            ),
            (
              region.start.map(|start| start.to_string()),
              region.end.map(|end| end.to_string()),
            ),
            self.encryption_scheme.clone(),
          )
        })
        .collect::<Result<Vec<Query>>>()
    } else {
      Ok(vec![set_query_builder(
        QueryBuilder::new(request, format),
        self.class,
        None::<String>,
        Self::join_vec(self.fields),
        (Self::join_vec(self.tags), Self::join_vec(self.notags)),
        (None::<String>, None::<String>),
        self.encryption_scheme,
      )?])
    }
  }

  fn join_vec(fields: Option<Vec<String>>) -> Option<String> {
    fields.map(|fields| fields.join(","))
  }
}

#[cfg(test)]
mod tests {
  use htsget_config::types::{Class, Format};

  use super::*;

  #[test]
  fn post_request_without_regions() {
    let request = Request::new_with_id("id".to_string());

    assert_eq!(
      PostRequest {
        format: Some("VCF".to_string()),
        class: Some("header".to_string()),
        fields: None,
        tags: None,
        notags: None,
        regions: None,
        encryption_scheme: None,
      }
      .get_queries(request.clone(), &Endpoint::Variants)
      .unwrap(),
      vec![Query::new("id", Format::Vcf, request).with_class(Class::Header)]
    );
  }

  #[test]
  fn post_request_with_one_region() {
    let request = Request::new_with_id("id".to_string());

    assert_eq!(
      PostRequest {
        format: Some("VCF".to_string()),
        class: Some("header".to_string()),
        fields: None,
        tags: None,
        notags: None,
        regions: Some(vec![Region {
          reference_name: "20".to_string(),
          start: Some(150),
          end: Some(153),
        }]),
        encryption_scheme: None,
      }
      .get_queries(request.clone(), &Endpoint::Variants)
      .unwrap(),
      vec![
        Query::new("id", Format::Vcf, request)
          .with_class(Class::Header)
          .with_reference_name("20".to_string())
          .with_start(150)
          .with_end(153)
      ]
    );
  }

  #[test]
  fn post_request_with_regions() {
    let request = Request::new_with_id("id".to_string());

    assert_eq!(
      PostRequest {
        format: Some("VCF".to_string()),
        class: Some("header".to_string()),
        fields: None,
        tags: None,
        notags: None,
        regions: Some(vec![
          Region {
            reference_name: "20".to_string(),
            start: Some(150),
            end: Some(153),
          },
          Region {
            reference_name: "11".to_string(),
            start: Some(152),
            end: Some(154),
          }
        ]),
        encryption_scheme: None,
      }
      .get_queries(request.clone(), &Endpoint::Variants)
      .unwrap(),
      vec![
        Query::new("id", Format::Vcf, request.clone())
          .with_class(Class::Header)
          .with_reference_name("20".to_string())
          .with_start(150)
          .with_end(153),
        Query::new("id", Format::Vcf, request)
          .with_class(Class::Header)
          .with_reference_name("11".to_string())
          .with_start(152)
          .with_end(154)
      ]
    );
  }
}
