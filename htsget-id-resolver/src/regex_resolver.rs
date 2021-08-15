use super::HtsGetIdResolver;
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

impl HtsGetIdResolver for RegexResolver {
  fn resolve_id(&self, id: &str) -> Option<String> {
    if self.regex.is_match(id) {
      Some(self.regex.replace(id, &self.replacement_string).to_string())
    } else {
      None
    }
  }
}
