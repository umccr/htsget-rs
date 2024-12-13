use htsget_config::config;
use htsget_config::types::Format;
use htsget_search::HtsGet;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::debug;
use tracing::instrument;

use crate::Endpoint;

const READS_FORMATS: [&str; 2] = ["BAM", "CRAM"];
const VARIANTS_FORMATS: [&str; 2] = ["VCF", "BCF"];

const HTSGET_GROUP: &str = "org.ga4gh";
const HTSGET_ARTIFACT: &str = "htsget";
const HTSGET_VERSION: &str = "1.3.0";

/// A struct representing the information that should be present in a service-info response.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
  #[serde(flatten)]
  pub fields: HashMap<String, Value>,
  #[serde(rename = "type")]
  pub service_type: Type,
  pub htsget: Htsget,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Type {
  pub group: String,
  pub artifact: String,
  pub version: String,
}

impl Default for Type {
  fn default() -> Self {
    Self {
      group: HTSGET_GROUP.to_string(),
      artifact: HTSGET_ARTIFACT.to_string(),
      version: HTSGET_VERSION.to_string(),
    }
  }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Htsget {
  pub datatype: String,
  pub formats: Vec<String>,
  pub fields_parameters_effective: bool,
  pub tags_parameters_effective: bool,
}

impl ServiceInfo {
  pub fn new(
    endpoint: Endpoint,
    supported_formats: &[Format],
    fields_effective: bool,
    tags_effective: bool,
    fields: HashMap<String, Value>,
  ) -> Self {
    let htsget_info = Htsget {
      datatype: match endpoint {
        Endpoint::Reads => "reads",
        Endpoint::Variants => "variants",
      }
      .to_string(),
      formats: supported_formats
        .iter()
        .map(|format| format.to_string())
        .filter(|format| match endpoint {
          Endpoint::Reads => READS_FORMATS.contains(&format.as_str()),
          Endpoint::Variants => VARIANTS_FORMATS.contains(&format.as_str()),
        })
        .collect(),
      fields_parameters_effective: fields_effective,
      tags_parameters_effective: tags_effective,
    };

    Self {
      fields,
      service_type: Default::default(),
      htsget: htsget_info,
    }
  }
}

#[instrument(level = "debug", skip_all)]
pub fn get_service_info_json(
  endpoint: Endpoint,
  searcher: impl HtsGet + Send + Sync + 'static,
  config: config::service_info::ServiceInfo,
) -> ServiceInfo {
  debug!(endpoint = ?endpoint,"getting service-info response for endpoint");
  ServiceInfo::new(
    endpoint,
    &searcher.get_supported_formats(),
    searcher.are_field_parameters_effective(),
    searcher.are_tag_parameters_effective(),
    config.into_inner(),
  )
}
