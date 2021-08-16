use std::sync::Arc;

use serde::{Deserialize, Serialize};

use htsget_search::htsget::{Format, HtsGet};

use crate::{Endpoint, READS_FORMATS, VARIANTS_FORMATS};

/// A struct representing the information that should be present in a service-info response
#[derive(Serialize, Deserialize)]
pub struct ServiceInfo {
  pub id: String,
  pub name: String,
  pub version: String,
  pub organization: ServiceInfoOrganization,
  #[serde(rename = "type")]
  pub service_type: ServiceInfoType,
  pub htsget: ServiceInfoHtsget,
  // The next fields aren't in the HtsGet specification, but were added
  // because they were present in the reference implementation and were deemed useful
  #[serde(rename = "contactUrl")]
  pub contact_url: String,
  #[serde(rename = "documentationUrl")]
  pub documentation_url: String,
  #[serde(rename = "createdAt")]
  pub created_at: String,
  #[serde(rename = "UpdatedAt")]
  pub updated_at: String,
  pub environment: String,
}

#[derive(Serialize, Deserialize)]
pub struct ServiceInfoOrganization {
  pub name: String,
  pub url: String,
}

#[derive(Serialize, Deserialize)]
pub struct ServiceInfoType {
  pub group: String,
  pub artifact: String,
  pub version: String,
}

#[derive(Serialize, Deserialize)]
pub struct ServiceInfoHtsget {
  pub datatype: String,
  pub formats: Vec<String>,
  #[serde(rename = "fieldsParametersEffective")]
  pub fields_parameters_effective: bool,
  #[serde(rename = "TagsParametersEffective")]
  pub tags_parameters_effective: bool,
}

pub fn get_service_info_with(
  endpoint: Endpoint,
  supported_formats: &[Format],
  fields_effective: bool,
  tags_effective: bool,
) -> ServiceInfo {
  let hstget_info = ServiceInfoHtsget {
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
  let type_info = ServiceInfoType {
    group: "org.ga4gh".to_string(),
    artifact: "htsget".to_string(),
    version: "1.3.0".to_string(),
  };
  let organization_info = ServiceInfoOrganization {
    name: "Snake oil".to_string(),
    url: "https://en.wikipedia.org/wiki/Snake_oil".to_string(),
  };
  ServiceInfo {
    id: "".to_string(),
    name: "HtsGet service".to_string(),
    version: "".to_string(),
    organization: organization_info,
    service_type: type_info,
    htsget: hstget_info,
    contact_url: "".to_string(),
    documentation_url: "https://github.com/umccr/htsget-rs/tree/main/htsget-http-actix".to_string(),
    created_at: "".to_string(),
    updated_at: "".to_string(),
    environment: "testing".to_string(),
  }
}

pub fn get_service_info_json(
  endpoint: Endpoint,
  searcher: Arc<impl HtsGet + Send + Sync + 'static>,
) -> ServiceInfo {
  get_service_info_with(
    endpoint,
    &searcher.get_supported_formats(),
    searcher.are_field_parameters_effective(),
    searcher.are_tag_parameters_effective(),
  )
}
