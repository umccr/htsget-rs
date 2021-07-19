use crate::{QueryBuilder, Result};
use htsget_search::htsget::Query;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct PostRequest {
  pub format: Option<String>,
  pub class: Option<String>,
  pub fields: Option<Vec<String>>,
  pub tags: Option<Vec<String>>,
  pub notags: Option<Vec<String>>,
  pub regions: Option<Vec<Region>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Region {
  #[serde(rename = "referenceName")]
  pub reference_name: String,
  pub start: Option<u32>,
  pub end: Option<u32>,
}

impl PostRequest {
  pub fn get_queries(self, id: impl Into<String>) -> Result<Vec<Query>> {
    if let Some(ref regions) = self.regions {
      let id = id.into();
      regions
        .iter()
        .map(|region| {
          Ok(
            self
              .get_base_query_builder(id.clone())?
              .add_reference_name(Some(region.reference_name.clone()))
              .add_range_from_u32(region.start, region.end)?
              .build(),
          )
        })
        .collect::<Result<Vec<Query>>>()
    } else {
      Ok(vec![self.get_base_query_builder(id)?.build()])
    }
  }

  fn get_base_query_builder(&self, id: impl Into<String>) -> Result<QueryBuilder> {
    Ok(
      QueryBuilder::new(Some(id.into()))?
        .add_format(self.format.clone())?
        .add_class(self.class.clone())?
        .add_fields_from_vec(self.fields.clone())
        .add_tags_from_vec(self.tags.clone(), self.notags.clone())?,
    )
  }
}
