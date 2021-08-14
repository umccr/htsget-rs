use super::HtsgetIdResolver;
use regex::{Error, Regex};

#[derive(Debug)]
pub struct RegexResolver {
  regex: Regex,
  replacement_string: String,
}

impl RegexResolver {
  pub fn new(regex: &str, replacement_string: &str) -> Result<Self, Error> {
    Ok(RegexResolver {
      regex: Regex::new(regex)?,
      replacement_string: replacement_string.to_string(),
    })
  }
}

impl HtsgetIdResolver for RegexResolver {
  fn resolve_id(&self, id: &str) -> String {
    id.to_string()
  }
}
