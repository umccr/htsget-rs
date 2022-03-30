use htsget_config::config::HtsgetConfig;
use htsget_search::htsget::blocking::HtsGet;

use crate::service_info::{fill_out_service_info_json, get_service_info_with};
use crate::{Endpoint, ServiceInfo};

pub fn get_service_info_json(
  endpoint: Endpoint,
  searcher: &impl HtsGet,
  config: &HtsgetConfig,
) -> ServiceInfo {
  fill_out_service_info_json(
    get_service_info_with(
      endpoint,
      &searcher.get_supported_formats(),
      searcher.are_field_parameters_effective(),
      searcher.are_tag_parameters_effective(),
    ),
    config,
  )
}
