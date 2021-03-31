use crate::htsget::{HtsGet, Query, Response, HtsGetError};

pub struct SimpleHtsGet {

}

impl SimpleHtsGet {
  pub fn new() -> Self {
    Self {}
  }
}

impl HtsGet for SimpleHtsGet {
  fn search(&self, query: Query) -> Result<Response, HtsGetError> {
      unimplemented!()
  }
}